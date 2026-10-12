//! 共享文本/集合工具 / Shared text & set utilities
//!
//! 流水线各步骤共用的相似度算法，消除重复实现。

use std::collections::HashSet;
use std::hash::{BuildHasher, Hash};

/// 计算两个集合的 Jaccard 相似系数 / Compute Jaccard similarity between two sets.
///
/// 返回 `intersection / union`，当两个集合均为空时返回 0.0。
/// Returns `intersection / union`; returns 0.0 when both sets are empty.
pub fn jaccard<T, S>(a: &HashSet<T, S>, b: &HashSet<T, S>) -> f64
where
    T: Eq + Hash,
    S: BuildHasher,
{
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_sets_have_jaccard_one() {
        let a: HashSet<&str> = ["x", "y", "z"].into_iter().collect();
        assert!((jaccard(&a, &a) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn disjoint_sets_have_jaccard_zero() {
        let a: HashSet<&str> = ["x"].into_iter().collect();
        let b: HashSet<&str> = ["y"].into_iter().collect();
        assert!(jaccard(&a, &b).abs() < f64::EPSILON);
    }

    #[test]
    fn both_empty_returns_zero() {
        let a: HashSet<&str> = HashSet::new();
        let b: HashSet<&str> = HashSet::new();
        assert!(jaccard(&a, &b).abs() < f64::EPSILON);
    }
}
