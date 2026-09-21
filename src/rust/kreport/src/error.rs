use std::io;

use thiserror::Error;

use crate::domain::ParseError;

/// A report could not be read or parsed.
///
/// The message identifies the report line. Use `std::error::Error::source()` to
/// inspect the underlying cause.
///
/// ```
/// use kreport::{load_kreport, Error};
/// match load_kreport(b"broken\n".as_slice(), Default::default()) {
///     Err(Error::Parse { line, source }) => {
///         assert_eq!(line, 1);
///         eprintln!("{source}");
///     }
///     Err(error) => return Err(error),
///     Ok(report) => println!("{} entries", report.len()),
/// }
/// # Ok::<(), Error>(())
/// ```
#[derive(Debug, Error)]
pub enum Error {
    /// Reading the report failed.
    #[error("Failed to read Kraken report at line {line}: {source}")]
    Read {
        /// The report line where reading failed.
        line: usize,
        /// The underlying input failure.
        #[source]
        source: io::Error,
    },
    /// Parsing a report entry failed.
    #[error("Failed to parse Kraken report at line {line}: {source}")]
    Parse {
        /// The report line containing the entry.
        line: usize,
        /// Details of the entry parsing failure.
        #[source]
        source: ParseError,
    },
}
