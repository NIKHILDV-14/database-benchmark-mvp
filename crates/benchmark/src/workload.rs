use dataset::model::Document;

/// A single query to execute against the database.
///
/// The adapter decides how to interpret the query string based on its own
/// capabilities (SQL, MQL, vector ANN, etc.).
#[derive(Debug, Clone)]
pub struct QuerySpec {
    /// Opaque query string handed to `DatabaseAdapter::execute`.
    pub query: String,
    /// Human-readable label used in result output.
    pub label: String,
}

impl QuerySpec {
    pub fn new(label: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            label: label.into(),
        }
    }
}

/// Defines the complete workload to run against a database.
///
/// A `Workload` is database-agnostic: it carries the documents to insert and
/// the queries to execute.  The engine feeds the same workload to every adapter.
#[derive(Debug, Clone)]
pub struct Workload {
    /// Human-readable name for this workload.
    pub name: String,
    /// Documents to be loaded via `DatabaseAdapter::load_dataset`.
    pub documents: Vec<Document>,
    /// Queries to execute via `DatabaseAdapter::execute`, in order.
    pub queries: Vec<QuerySpec>,
    /// Document IDs to delete in the cleanup phase.
    pub delete_ids: Vec<String>,
}

impl Workload {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            documents: Vec::new(),
            queries: Vec::new(),
            delete_ids: Vec::new(),
        }
    }

    pub fn with_documents(mut self, docs: Vec<Document>) -> Self {
        self.documents = docs;
        self
    }

    pub fn with_queries(mut self, queries: Vec<QuerySpec>) -> Self {
        self.queries = queries;
        self
    }

    pub fn with_delete_ids(mut self, ids: Vec<String>) -> Self {
        self.delete_ids = ids;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_doc(id: &str) -> Document {
        Document {
            id: id.to_string(),
            title: None,
            text: "test".to_string(),
            metadata: json!({}),
        }
    }

    #[test]
    fn test_workload_builder() {
        let wl = Workload::new("smoke-test")
            .with_documents(vec![make_doc("d1"), make_doc("d2")])
            .with_queries(vec![QuerySpec::new("q1", "SELECT 1")])
            .with_delete_ids(vec!["d1".to_string()]);

        assert_eq!(wl.name, "smoke-test");
        assert_eq!(wl.documents.len(), 2);
        assert_eq!(wl.queries.len(), 1);
        assert_eq!(wl.delete_ids, vec!["d1".to_string()]);
    }

    #[test]
    fn test_query_spec() {
        let qs = QuerySpec::new("my-query", "SELECT 42");
        assert_eq!(qs.label, "my-query");
        assert_eq!(qs.query, "SELECT 42");
    }
}
