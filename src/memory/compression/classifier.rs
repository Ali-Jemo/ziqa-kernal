use super::tier::CompressionTier;

/// Result of analyzing a page's content.
#[derive(Debug, Clone)]
pub struct PageClassification {
    /// Shannon entropy (bits/byte, 0.0–8.0)
    pub entropy: f64,
    /// Whether the page should be compressed
    pub should_compress: bool,
    /// Recommended compression strategy
    pub recommended_tier: CompressionTier,
    /// Estimated compression ratio (uncompressed / compressed)
    pub estimated_ratio: f64,
    /// Content pattern detected
    pub pattern: PagePattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagePattern {
    /// All bytes are zero
    AllZeros,
    /// Very short repeating pattern (e.g., 0x00 0x40 0x00 0x40 …)
    ShortPattern { period: usize },
    /// Text or code-like content (ASCII range)
    TextOrCode,
    /// Random-looking / high-entropy data
    Random,
    /// Mixed / generic content
    Mixed,
}

/// Heuristic zero-run threshold: if this many consecutive zero bytes
/// appear, the page has significant sparsity.
const ZERO_RUN_THRESHOLD: usize = 64;
/// Entropy below this → compressible with good ratio.
const LOW_ENTROPY_THRESHOLD: f64 = 5.0;
/// Entropy above this → likely incompressible.
const HIGH_ENTROPY_THRESHOLD: f64 = 7.5;

/// Analyse a page of memory and return a classification.
pub fn classify(data: &[u8]) -> PageClassification {
    let entropy = super::engine::AdaptiveCompressor::shannon_entropy(data);
    let pattern = detect_pattern(data);
    let (should_compress, recommended_tier, estimated_ratio) =
        decide(entropy, &pattern);

    PageClassification {
        entropy,
        should_compress,
        recommended_tier,
        estimated_ratio,
        pattern,
    }
}

fn detect_pattern(data: &[u8]) -> PagePattern {
    if data.is_empty() {
        return PagePattern::AllZeros;
    }

    // All zeros?
    if data.iter().all(|&b| b == 0) {
        return PagePattern::AllZeros;
    }

    // Short repeating pattern? Check first few bytes repeat.
    for period in 1..=16 {
        if period > data.len() {
            break;
        }
        let pattern = &data[..period];
        let mut matches = true;
        for chunk in data.chunks(period) {
            if chunk.len() < period {
                // Partial chunk at end – check what's there
                if !chunk.iter().zip(pattern[..chunk.len()].iter()).all(|(a, b)| a == b) {
                    matches = false;
                    break;
                }
                break;
            }
            if chunk != pattern {
                matches = false;
                break;
            }
        }
        if matches {
            return PagePattern::ShortPattern { period };
        }
    }

    // Check for long zero runs (sparse pages)
    let longest_zero_run = data
        .split(|&b| b != 0)
        .map(|run| run.len())
        .max()
        .unwrap_or(0);
    if longest_zero_run >= ZERO_RUN_THRESHOLD {
        return PagePattern::Mixed;
    }

    // Heuristic: mostly printable ASCII + common control chars = text/code
    let ascii_or_code = data
        .iter()
        .filter(|&&b| b.is_ascii_graphic() || b.is_ascii_whitespace() || b == 0)
        .count();
    if ascii_or_code as f64 / data.len() as f64 > 0.85 {
        return PagePattern::TextOrCode;
    }

    // If entropy is very high, treat as random
    if crate::memory::compression::engine::AdaptiveCompressor::shannon_entropy(data) > HIGH_ENTROPY_THRESHOLD {
        return PagePattern::Random;
    }

    PagePattern::Mixed
}

fn decide(entropy: f64, pattern: &PagePattern) -> (bool, CompressionTier, f64) {
    match pattern {
        PagePattern::AllZeros => {
            (true, CompressionTier::Lz4, 256.0)
        }
        PagePattern::ShortPattern { period: _ } => {
            (true, CompressionTier::Lz4, 16.0)
        }
        PagePattern::TextOrCode => {
            (true, CompressionTier::Lz4, 4.0)
        }
        PagePattern::Random => {
            (false, CompressionTier::Uncompressed, 1.0)
        }
        PagePattern::Mixed => {
            if entropy <= LOW_ENTROPY_THRESHOLD {
                (true, CompressionTier::Lz4, 3.0)
            } else if entropy <= HIGH_ENTROPY_THRESHOLD {
                (true, CompressionTier::Lz4, 1.5)
            } else {
                (false, CompressionTier::Uncompressed, 1.0)
            }
        }
    }
}

/// Convenience: decide whether a page is compressible without full classification.
pub fn is_compressible(data: &[u8]) -> bool {
    classify(data).should_compress
}
