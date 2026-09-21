use std::{
    collections::BTreeMap,
    error::Error,
    panic::{catch_unwind, AssertUnwindSafe},
    thread,
};

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use rayon::{ThreadPool, ThreadPoolBuilder};

use crate::ports::{RecordExecutor, RecordSink, RecordSource, Result, WorkflowError};

/// Process batches concurrently and write their results in input order.
///
/// A bounded run holds at most `queue_batches + threads + 1` batches across
/// reading, processing, and ordered output. `None` allows an unbounded queue.
pub struct OrderedExecutor {
    batch_size: usize,
    queue_batches: Option<usize>,
    pool: ThreadPool,
}

struct Batch<T> {
    ordinal: usize,
    offset: usize,
    records: Result<Vec<T>>,
}

struct Completed<T> {
    ordinal: usize,
    records: Result<Vec<T>>,
}

impl OrderedExecutor {
    pub fn new(batch_size: usize, queue_batches: Option<usize>, threads: usize) -> Result<Self> {
        // A zero batch historically sends each record immediately.
        let batch_size = batch_size.max(1);
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .build()
            .map_err(|error| WorkflowError::operation("create processing workers", error))?;
        Ok(Self {
            batch_size,
            queue_batches,
            pool,
        })
    }
}

impl RecordExecutor for OrderedExecutor {
    fn execute<S, W, F, T, U, E>(&self, source: S, sink: &mut W, transform: F) -> Result<()>
    where
        S: RecordSource<Record = T> + Send,
        W: RecordSink<U>,
        F: Fn(T) -> std::result::Result<Option<U>, E> + Sync,
        T: Send,
        U: Send,
        E: Error + Send + Sync + 'static,
    {
        let (input_tx, input_rx) = channel(self.queue_batches);
        let (output_tx, output_rx) = channel(self.queue_batches);
        // Credits are returned only after writing, so a slow earlier batch
        // cannot cause unbounded result reordering.
        let credits = self.queue_batches.map(|capacity| {
            let capacity = capacity
                .saturating_add(self.pool.current_num_threads())
                .saturating_add(1);
            let (sender, receiver) = bounded(capacity);
            for _ in 0..capacity {
                sender.send(()).expect("credit receiver is alive");
            }
            (sender, receiver)
        });
        let (credit_tx, credit_rx) = credits.map_or((None, None), |(tx, rx)| (Some(tx), Some(rx)));
        thread::scope(|threads| {
            let reader =
                threads.spawn(move || read_batches(source, input_tx, credit_rx, self.batch_size));
            let result = self.pool.in_place_scope(|workers| {
                for _ in 0..self.pool.current_num_threads() {
                    let (input, output, transform) =
                        (input_rx.clone(), output_tx.clone(), &transform);
                    workers.spawn(move |_| process_batches(input, output, transform));
                }
                drop(input_rx);
                drop(output_tx);
                let result = write_batches(&output_rx, sink, credit_tx.as_ref());
                // Both queues must disconnect before joining. This releases readers
                // and workers blocked on any stage, including rendezvous queues.
                drop(credit_tx);
                drop(output_rx);
                result
            });
            let joined = reader
                .join()
                .map_err(|_| WorkflowError::WorkerPanic("input"));
            result?;
            joined
        })
    }
}

fn channel<T>(capacity: Option<usize>) -> (Sender<T>, Receiver<T>) {
    capacity.map_or_else(unbounded, bounded)
}

fn read_batch<S: RecordSource>(source: &mut S, size: usize) -> Result<Vec<S::Record>> {
    let mut records = Vec::with_capacity(size);
    while records.len() < size {
        let Some(record) = source.next_record()? else {
            break;
        };
        records.push(record);
    }
    Ok(records)
}

fn read_batches<S: RecordSource>(
    mut source: S,
    sender: Sender<Batch<S::Record>>,
    credits: Option<Receiver<()>>,
    size: usize,
) {
    let (mut ordinal, mut offset) = (0, 0);
    loop {
        if credits
            .as_ref()
            .is_some_and(|credits| credits.recv().is_err())
        {
            return;
        }
        let records = read_batch(&mut source, size);
        let count = records.as_ref().map_or(0, Vec::len);
        let finished = count == 0;
        if matches!(&records, Ok(records) if records.is_empty()) {
            return;
        }
        if sender
            .send(Batch {
                ordinal,
                offset,
                records,
            })
            .is_err()
            || finished
        {
            return;
        }
        ordinal += 1;
        offset += count;
    }
}

fn process_batches<T, U, E, F>(
    input: Receiver<Batch<T>>,
    output: Sender<Completed<U>>,
    transform: &F,
) where
    F: Fn(T) -> std::result::Result<Option<U>, E>,
    E: Error + Send + Sync + 'static,
{
    for batch in input {
        let records = catch_unwind(AssertUnwindSafe(|| {
            batch
                .records?
                .into_iter()
                .enumerate()
                .filter_map(|(index, record)| {
                    transform(record)
                        .map_err(|error| {
                            WorkflowError::operation(
                                format!("record {}", batch.offset + index + 1),
                                error,
                            )
                        })
                        .transpose()
                })
                .collect::<Result<Vec<_>>>()
        }))
        .unwrap_or_else(|_| Err(WorkflowError::WorkerPanic("processing")));
        if output
            .send(Completed {
                ordinal: batch.ordinal,
                records,
            })
            .is_err()
        {
            return;
        }
    }
}

fn write_batches<T, W: RecordSink<T>>(
    input: &Receiver<Completed<T>>,
    sink: &mut W,
    credits: Option<&Sender<()>>,
) -> Result<()> {
    let mut next = 0;
    let mut pending = BTreeMap::new();
    for batch in input {
        pending.insert(batch.ordinal, batch.records);
        while let Some(records) = pending.remove(&next) {
            for record in records? {
                sink.write_record(record)?;
            }
            next += 1;
            if let Some(credits) = credits {
                let _ = credits.send(());
            }
        }
    }
    sink.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{convert::Infallible, io, time::Duration};

    struct Source {
        records: std::ops::Range<usize>,
        fail_at: Option<usize>,
    }
    impl RecordSource for Source {
        type Record = usize;
        fn next_record(&mut self) -> Result<Option<usize>> {
            let next = self.records.next();
            if next.is_some() && next == self.fail_at {
                return Err(WorkflowError::InvalidRequest("input failed".into()));
            }
            Ok(next)
        }
    }

    #[derive(Default)]
    struct Sink {
        records: Vec<usize>,
        fail_write: bool,
        fail_finish: bool,
        finished: bool,
    }
    impl RecordSink<usize> for Sink {
        fn write_record(&mut self, record: usize) -> Result<()> {
            if self.fail_write {
                return Err(WorkflowError::InvalidRequest("output failed".into()));
            }
            self.records.push(record);
            Ok(())
        }
        fn finish(&mut self) -> Result<()> {
            self.finished = true;
            if self.fail_finish {
                Err(WorkflowError::InvalidRequest("flush failed".into()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn preserves_order_with_parallel_transforms_and_filtering() {
        for queue in [Some(0), Some(1), None] {
            let executor = OrderedExecutor::new(8, queue, 4).unwrap();
            let mut sink = Sink::default();
            executor
                .execute(
                    Source {
                        records: 0..40,
                        fail_at: None,
                    },
                    &mut sink,
                    |record| {
                        if record % 8 == 0 {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Ok::<_, Infallible>((record % 2 == 0).then_some(record))
                    },
                )
                .unwrap();
            assert_eq!(sink.records, (0..40).step_by(2).collect::<Vec<_>>());
            assert!(sink.finished);
        }
    }

    #[test]
    fn failures_cancel_a_reader_blocked_on_a_bounded_queue() {
        for queue in [Some(0), Some(1)] {
            let executor = OrderedExecutor::new(2, queue, 2).unwrap();
            let error = executor
                .execute(
                    Source {
                        records: 0..10000,
                        fail_at: None,
                    },
                    &mut Sink::default(),
                    |_| Err::<Option<usize>, _>(io::Error::other("transform failed")),
                )
                .unwrap_err();
            assert!(error.to_string().contains("record 1: transform failed"));
            let mut sink = Sink {
                fail_write: true,
                ..Default::default()
            };
            let error = executor
                .execute(
                    Source {
                        records: 0..10000,
                        fail_at: None,
                    },
                    &mut sink,
                    |record| Ok::<_, Infallible>(Some(record)),
                )
                .unwrap_err();
            assert_eq!(error.to_string(), "output failed");
        }
    }

    #[test]
    fn propagates_input_and_flush_failures() {
        let executor = OrderedExecutor::new(2, Some(1), 1).unwrap();
        let error = executor
            .execute(
                Source {
                    records: 0..5,
                    fail_at: Some(3),
                },
                &mut Sink::default(),
                |record| Ok::<_, Infallible>(Some(record)),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "input failed");
        let mut sink = Sink {
            fail_finish: true,
            ..Default::default()
        };
        let error = executor
            .execute(
                Source {
                    records: 0..0,
                    fail_at: None,
                },
                &mut sink,
                |record| Ok::<_, Infallible>(Some(record)),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "flush failed");
    }

    #[test]
    fn reports_worker_panics_and_accepts_zero_batch_size() {
        assert!(OrderedExecutor::new(0, Some(1), 1).is_ok());
        let executor = OrderedExecutor::new(2, Some(0), 2).unwrap();
        let error = executor
            .execute(
                Source {
                    records: 0..100,
                    fail_at: None,
                },
                &mut Sink::default(),
                |_| -> std::result::Result<Option<usize>, Infallible> { panic!("worker failure") },
            )
            .unwrap_err();
        assert!(matches!(error, WorkflowError::WorkerPanic("processing")));
    }
}
