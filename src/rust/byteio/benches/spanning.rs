use std::{
    env,
    hint::black_box,
    io::{self, Read},
    time::Instant,
};

use mire_byteio::{BytesReader, DelimPolicy};

const BUFFER_SIZE: usize = 8192;
const INPUT_SIZE: usize = 32 * 1024 * 1024;

struct ChunkedRead<'a> {
    input: &'a [u8],
    max_read: usize,
}

impl Read for ChunkedRead<'_> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        let length = destination.len().min(self.max_read);
        self.input.read(&mut destination[..length])
    }
}

fn run(input: &[u8], max_read: usize, retain: bool, validate: bool) -> io::Result<(usize, usize)> {
    let source = ChunkedRead { input, max_read };
    let mut reader = BytesReader::with_capacity(BUFFER_SIZE, source);
    let mut held = Vec::new();
    let mut output_bytes = 0;
    let mut lines = 0;
    while let Some(line) = reader.read_line(DelimPolicy::Exclude)? {
        if validate {
            // Validate every delivered byte outside the timed runs.
            assert!(line.iter().all(|&byte| byte == 0xff));
        }
        let bytes = black_box(line.as_ref());
        black_box(bytes.first());
        black_box(bytes.last());
        output_bytes += bytes.len();
        lines += 1;
        if retain {
            held.push(line);
        }
    }
    // Keep all retained results alive until reading has finished. Their
    // destruction, like the reader's destruction, is included in the timing.
    black_box(&held);
    Ok((output_bytes, lines))
}

fn main() -> io::Result<()> {
    let dataset_filter = env::var("BYTEIO_BENCH_DATASET").ok();
    let retention_filter = env::var("BYTEIO_BENCH_RETENTION").ok();
    println!(
        "dataset,retention,buffer_size,max_read,line_bytes,input_bytes,output_bytes,lines,sample,nanoseconds"
    );
    // Lengths include CRLF. Aligned lines make the number of blocks explicit;
    // the unaligned case also exercises first blocks of varying lengths.
    for (dataset, line_bytes, max_read) in [
        ("buffered_control", 64, BUFFER_SIZE),
        ("unaligned", 8202, BUFFER_SIZE),
        ("two_blocks", 2 * BUFFER_SIZE, BUFFER_SIZE),
        ("four_blocks", 4 * BUFFER_SIZE, BUFFER_SIZE),
        ("long", 1024 * 1024, BUFFER_SIZE),
        ("short_reads", 2048, 512),
    ] {
        if dataset_filter
            .as_deref()
            .is_some_and(|name| name != dataset)
        {
            continue;
        }
        let mut line = vec![0xff; line_bytes - 2];
        line.extend_from_slice(b"\r\n");
        let line_count = INPUT_SIZE / line_bytes;
        let input = line.repeat(line_count);
        let expected = ((line_bytes - 2) * line_count, line_count);
        for retain in [false, true] {
            let retention = if retain { "retain" } else { "immediate" };
            if retention_filter
                .as_deref()
                .is_some_and(|name| name != retention)
            {
                continue;
            }
            assert_eq!(run(&input, max_read, retain, true)?, expected);
            // Use the same non-validating path for the warm-up and samples.
            assert_eq!(run(&input, max_read, retain, false)?, expected);
            for sample in 1..=5 {
                let started = Instant::now();
                let (output_bytes, lines) = run(&input, max_read, retain, false)?;
                let elapsed = started.elapsed().as_nanos();
                assert_eq!((output_bytes, lines), expected);
                println!(
                    "{dataset},{retention},{BUFFER_SIZE},{max_read},{line_bytes},{},{output_bytes},{lines},{sample},{elapsed}",
                    input.len()
                );
            }
        }
    }
    Ok(())
}
