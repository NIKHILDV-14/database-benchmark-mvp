use std::time::Duration;

/// Calculate the mean (average) duration from a slice of durations.
pub fn mean(durations: &[Duration]) -> Option<Duration> {
    if durations.is_empty() {
        return None;
    }
    let sum: Duration = durations.iter().sum();
    Some(sum / durations.len() as u32)
}

/// Calculate the median duration from a slice of durations.
pub fn median(durations: &[Duration]) -> Option<Duration> {
    if durations.is_empty() {
        return None;
    }
    let mut sorted = durations.to_vec();
    sorted.sort();

    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        Some((sorted[mid - 1] + sorted[mid]) / 2)
    } else {
        Some(sorted[mid])
    }
}

/// Calculate the minimum duration from a slice of durations.
pub fn min(durations: &[Duration]) -> Option<Duration> {
    durations.iter().min().copied()
}

/// Calculate the maximum duration from a slice of durations.
pub fn max(durations: &[Duration]) -> Option<Duration> {
    durations.iter().max().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_inputs() {
        let empty: &[Duration] = &[];
        assert_eq!(mean(empty), None);
        assert_eq!(median(empty), None);
        assert_eq!(min(empty), None);
        assert_eq!(max(empty), None);
    }

    #[test]
    fn test_statistics_calculations() {
        let durations = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
            Duration::from_millis(100),
        ];

        assert_eq!(mean(&durations), Some(Duration::from_millis(40)));
        assert_eq!(median(&durations), Some(Duration::from_millis(30)));
        assert_eq!(min(&durations), Some(Duration::from_millis(10)));
        assert_eq!(max(&durations), Some(Duration::from_millis(100)));
    }

    #[test]
    fn test_median_even_length() {
        let durations = vec![
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(30),
            Duration::from_millis(40),
        ];
        // (20 + 30) / 2 = 25
        assert_eq!(median(&durations), Some(Duration::from_millis(25)));
    }
}
