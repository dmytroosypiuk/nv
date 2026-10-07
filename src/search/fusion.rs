//! Reciprocal Rank Fusion: one ranking out of several.

/// The usual RRF constant; a bigger `k` makes top ranks count less.
pub const RRF_K: f64 = 60.0;

/// Combines rankings of note IDs (best first) into one, best first.
///
/// Each ranking gives a note `1 / (k + rank)`. Scores are not compared between rankers,
/// only ranks, so keyword and vector results mix fairly.
pub fn rrf(rankings: &[&[i64]], k: f64) -> Vec<i64> {
    // In order of first appearance, so that equal scores keep the first ranking's order.
    let mut scores: Vec<(i64, f64)> = Vec::new();
    for ranking in rankings {
        for (index, &id) in ranking.iter().enumerate() {
            let score = 1.0 / (k + index as f64 + 1.0);
            match scores.iter_mut().find(|(seen, _)| *seen == id) {
                Some((_, total)) => *total += score,
                None => scores.push((id, score)),
            }
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    scores.into_iter().map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_combines_keyword_and_vector_ranks() {
        let keyword = [1, 2, 3];
        let vector = [3, 1, 4];

        // 1: 1/61 + 1/62, 3: 1/63 + 1/61, 2: 1/62, 4: 1/63
        assert_eq!(rrf(&[&keyword, &vector], RRF_K), [1, 3, 2, 4]);
    }

    #[test]
    fn note_found_by_both_rankers_beats_note_found_by_one() {
        let keyword = [10, 20, 30];
        let vector = [40, 50, 30];

        assert_eq!(rrf(&[&keyword, &vector], RRF_K)[0], 30);
    }

    #[test]
    fn rrf_of_one_ranking_keeps_its_order() {
        assert_eq!(rrf(&[&[7, 3, 9]], RRF_K), [7, 3, 9]);
        assert_eq!(rrf(&[&[7, 3, 9], &[]], RRF_K), [7, 3, 9]);
        assert!(rrf(&[], RRF_K).is_empty());
    }

    #[test]
    fn equal_scores_keep_the_order_of_the_first_ranking() {
        assert_eq!(rrf(&[&[1, 2], &[2, 1]], RRF_K), [1, 2]);
        assert_eq!(rrf(&[&[5], &[6]], RRF_K), [5, 6]);
    }
}
