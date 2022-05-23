use super::QueryResult;

pub fn best_results(results: Vec<QueryResult>) -> Vec<QueryResult> {
    results
        .iter()
        .filter(|r| {
            if r.score() >= 75 {
                true
            } else {
                results
                    .iter()
                    .find(|r2| {
                        r2.id() != r.id()
                            && (r2.artist() == r.artist() || r2.title() == r.title())
                            && (90u8..=100).contains(&(r.score() + r2.score()))
                    })
                    .map(|v| {
                        log::debug!("Result match: {r:?} - {v:?}");
                        v
                    })
                    .is_some()
            }
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::emysound::matcher::best_results;
    use crate::emysound::QueryResult;

    #[test]
    fn test() {
        let make_result = |coverage: f64, artist: &str, title: &str| QueryResult {
            id: Uuid::new_v4(),
            coverage: coverage as f32,
            artist: Some(artist.to_owned()),
            title: Some(title.to_owned()),
        };
        let single = make_result(0.9, "Artist", "Title");
        let pair_a = make_result(0.6, "A", "B");
        let pair_b = make_result(0.3, "A", "B");

        println!("{:?}", best_results(vec![single, pair_a, pair_b]));
    }
}
