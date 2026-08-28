pub fn next_runtime_generation(current: u64) -> u64 {
    current.wrapping_add(1).max(1)
}

pub fn accepts_runtime_result(active: u64, incoming: u64) -> bool {
    active == incoming
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_runtime_replacement_rejects_every_prior_worker_result() {
        let stock = 1;
        let dashboard = next_runtime_generation(stock);
        assert!(!accepts_runtime_result(dashboard, stock));
        assert!(accepts_runtime_result(dashboard, dashboard));
        assert_eq!(next_runtime_generation(u64::MAX), 1);
    }
}
