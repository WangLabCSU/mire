use std::{
    hint::black_box,
    io::{self, BufReader},
    time::Instant,
};

use mire_byteio::{BytesReader, DelimPolicy, SliceReader, VecReader};

const CAPACITY: usize = 8192;

fn inspect(bytes: &[u8]) -> usize {
    let bytes = black_box(bytes);
    black_box(bytes.first());
    bytes.len()
}

fn run_vec(input: &[u8], retain: bool) -> io::Result<usize> {
    let mut reader = VecReader::new(BufReader::with_capacity(CAPACITY, input));
    let mut held = Vec::new();
    let mut total = 0;
    while let Some(line) = reader.read_line(DelimPolicy::Exclude)? {
        total += inspect(&line);
        if retain {
            held.push(line);
        }
    }
    black_box(&held);
    Ok(total)
}

fn run_slice(input: &[u8], retain: bool) -> io::Result<usize> {
    let mut reader = SliceReader::new(BufReader::with_capacity(CAPACITY, input));
    let mut held = Vec::new();
    let mut total = 0;
    while let Some(line) = reader.read_line(DelimPolicy::Exclude)? {
        total += inspect(line);
        if retain {
            held.push(line.to_vec());
        }
    }
    black_box(&held);
    Ok(total)
}

fn run_bytes(input: &[u8], retain: bool) -> io::Result<usize> {
    let mut reader = BytesReader::with_capacity(CAPACITY, input);
    let mut held = Vec::new();
    let mut total = 0;
    while let Some(line) = reader.read_line(DelimPolicy::Exclude)? {
        total += inspect(&line);
        if retain {
            held.push(line);
        }
    }
    black_box(&held);
    Ok(total)
}

fn main() -> io::Result<()> {
    println!("reader,dataset,retention,input_bytes,output_bytes,sample,nanoseconds");
    for (dataset, length) in [("short", 80), ("boundary", 8200), ("long", 1_048_587)] {
        let mut line = vec![0xff; length];
        line.extend_from_slice(b"\r\n");
        let input = line.repeat((8 * 1024 * 1024 / line.len()).max(1));
        for retain in [false, true] {
            for reader in ["slice", "vec", "bytes"] {
                for sample in 0..=5 {
                    let started = Instant::now();
                    let count = match reader {
                        "slice" => run_slice(&input, retain)?,
                        "vec" => run_vec(&input, retain)?,
                        "bytes" => run_bytes(&input, retain)?,
                        _ => unreachable!(),
                    };
                    let elapsed = started.elapsed().as_nanos();
                    assert_eq!(count, length * (input.len() / line.len()));
                    if sample != 0 {
                        let retention = if retain { "retain" } else { "immediate" };
                        println!(
                            "{reader},{dataset},{retention},{},{count},{sample},{elapsed}",
                            input.len()
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
