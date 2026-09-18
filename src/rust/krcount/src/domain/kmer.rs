use bytes::Bytes;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum KmerError {
    #[error("Invalid LCA token '{0}', expected taxid:count")]
    InvalidToken(String),
    #[error("LCA k-mer count overflow")]
    CountOverflow,
    #[error("Invalid total kmer count: {count}, sequence length: {length}")]
    InvalidCount { count: usize, length: usize },
    #[error("Mismatched LCA/sequence format")]
    ReadLayout,
}

/// A k-mer count has different units from a k-mer's nucleotide length.
#[derive(Debug, PartialEq, Eq)]
struct KmerCount(usize);

impl KmerCount {
    fn parse(lca: &[u8]) -> Result<Self, KmerError> {
        let mut total = 0usize;
        for token in lca.trim_ascii().split(|byte| *byte == b' ') {
            let invalid = || KmerError::InvalidToken(String::from_utf8_lossy(token).into_owned());
            let separator = token
                .iter()
                .position(|byte| *byte == b':')
                .ok_or_else(invalid)?;
            let count = std::str::from_utf8(&token[separator + 1..])
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or_else(invalid)?;
            total = total.checked_add(count).ok_or(KmerError::CountOverflow)?;
        }
        Ok(Self(total))
    }

    fn length_in(&self, sequence: &[u8]) -> Result<usize, KmerError> {
        if self.0 == 0 || self.0 > sequence.len() {
            return Err(KmerError::InvalidCount {
                count: self.0,
                length: sequence.len(),
            });
        }
        Ok(sequence.len() - self.0 + 1)
    }
}

fn extract_single(lca: &[u8], sequence: &[u8]) -> Result<Vec<Bytes>, KmerError> {
    let length = KmerCount::parse(lca)?.length_in(sequence)?;
    let sequence = Bytes::copy_from_slice(sequence);
    Ok(sequence
        .windows(length)
        .map(|window| sequence.slice_ref(window))
        .collect())
}

pub(crate) fn extract_kmers(lca: &[u8], sequences: &[&[u8]]) -> Result<Vec<Bytes>, KmerError> {
    let separator = lca.windows(3).position(|token| token == b"|:|");
    match (sequences, separator) {
        ([sequence], None) => extract_single(lca, sequence),
        ([first, second], Some(separator)) => {
            let mut kmers = extract_single(&lca[..separator], first)?;
            kmers.extend(extract_single(&lca[separator + 3..], second)?);
            Ok(kmers)
        }
        _ => Err(KmerError::ReadLayout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_lca_keeps_the_first_base_of_read2() {
        let kmers = extract_kmers(b"2:2 |:| 2:2", &[b"ACGT", b"TGCA"]).unwrap();
        assert_eq!(kmers, [b"ACG".as_slice(), b"CGT", b"TGC", b"GCA"]);
    }

    #[test]
    fn validates_counts_and_overflow_without_panicking() {
        for lca in [b"2:0".as_slice(), b"2:9", b"2:", b"2:x", b"bad", b""] {
            assert!(extract_kmers(lca, &[b"ACGT"]).is_err());
        }
        let lca = format!("2:{} 2:1", usize::MAX);
        assert!(matches!(
            extract_kmers(lca.as_bytes(), &[b"ACGT"]),
            Err(KmerError::CountOverflow)
        ));
        assert!(matches!(
            extract_kmers(b"2:1 |:| 2:1", &[b"ACGT"]),
            Err(KmerError::ReadLayout)
        ));
    }
}
