use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TestRequestRecord {
    pub(crate) method: &'static str,
    pub(crate) route: String,
    pub(crate) transaction_id: String,
    pub(crate) body_hash_hex: String,
    pub(crate) attempt_number: usize,
    pub(crate) outcome: &'static str,
}

#[derive(Clone, Default)]
pub(crate) struct TestRequestLedger {
    records: Arc<Mutex<Vec<TestRequestRecord>>>,
}

impl TestRequestLedger {
    pub(crate) fn record(
        &self,
        method: &'static str,
        route: impl Into<String>,
        transaction_id: impl Into<String>,
        body: &[u8],
        outcome: &'static str,
    ) {
        let route = route.into();
        let transaction_id = transaction_id.into();
        let mut records = self.records.lock().unwrap();
        let attempt_number = records
            .iter()
            .filter(|record| {
                record.method == method
                    && record.route == route
                    && record.transaction_id == transaction_id
            })
            .count()
            + 1;
        records.push(TestRequestRecord {
            method,
            route,
            transaction_id,
            body_hash_hex: blake3::hash(body).to_hex().to_string(),
            attempt_number,
            outcome,
        });
    }

    pub(crate) fn snapshot(&self) -> Vec<TestRequestRecord> {
        self.records.lock().unwrap().clone()
    }

    pub(crate) fn assert_single_post_for_intent(&self, transaction_id: &str, expected_body: &[u8]) {
        let posts: Vec<_> = self
            .snapshot()
            .into_iter()
            .filter(|record| record.method == "POST" && record.transaction_id == transaction_id)
            .collect();
        assert_eq!(posts.len(), 1, "a user intent must issue exactly one POST");
        assert_eq!(posts[0].route, "/transactions");
        assert_eq!(posts[0].attempt_number, 1);
        assert_eq!(
            posts[0].body_hash_hex,
            blake3::hash(expected_body).to_hex().to_string()
        );
    }
}
