use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Base58 alphabet (Bitcoin-style, excludes 0, O, I, l to avoid confusion)
const BASE58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Encode a number to base58 string
pub fn to_base58(mut num: u64) -> String {
    if num == 0 {
        return "1".to_string();
    }

    let mut result = Vec::new();
    while num > 0 {
        result.push(BASE58_ALPHABET[(num % 58) as usize]);
        num /= 58;
    }

    result.reverse();
    String::from_utf8(result).unwrap()
}

/// Hash any hashable content and return base58 representation
pub fn hash_to_base58<T: Hash>(content: &T) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    to_base58(hasher.finish())
}

/// Hash any hashable content and return binary u64 hash
pub fn hash_to_u64<T: Hash>(content: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Get first N characters of base58 hash for compact representation
pub fn hash_prefix<T: Hash>(content: &T, len: usize) -> String {
    let full_hash = hash_to_base58(content);
    if full_hash.len() >= len {
        full_hash[..len].to_string()
    } else {
        format!("{:1<width$}", full_hash, width = len) // pad with '1' if needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base58_encoding() {
        assert_eq!(to_base58(0), "1");
        assert_eq!(to_base58(57), "z");
        assert_eq!(to_base58(58), "21");
    }

    #[test]
    fn test_hash_functions() {
        let content = "test content";
        let hash1 = hash_to_base58(&content);
        let hash2 = hash_to_base58(&content);
        assert_eq!(hash1, hash2); // Same content should produce same hash

        let prefix = hash_prefix(&content, 8);
        assert_eq!(prefix.len(), 8);
    }
}
