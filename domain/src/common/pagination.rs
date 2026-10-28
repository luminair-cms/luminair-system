use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Pagination Parameters
// ---------------------------------------------------------------------------

/// Pagination parameters specifying page number and page size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
}

impl Pagination {
    /// Creates a new `Pagination` specification.
    #[must_use]
    pub const fn new(page: u32, page_size: u32) -> Self {
        Self { page, page_size }
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 25,
        }
    }
}

// ---------------------------------------------------------------------------
// Page Container
// ---------------------------------------------------------------------------

/// Generic paginated collection response container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

impl<T> Page<T> {
    /// Constructs a new `Page` container.
    #[must_use]
    pub const fn new(items: Vec<T>, total: u64, page: u32, page_size: u32) -> Self {
        Self {
            items,
            total,
            page,
            page_size,
        }
    }

    /// Constructs an empty `Page` for the given pagination parameters.
    #[must_use]
    pub fn empty(pagination: Pagination) -> Self {
        Self {
            items: Vec::new(),
            total: 0,
            page: pagination.page,
            page_size: pagination.page_size,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_default() {
        let default = Pagination::default();
        assert_eq!(default.page, 1);
        assert_eq!(default.page_size, 25);
    }

    #[test]
    fn test_pagination_new() {
        let p = Pagination::new(2, 50);
        assert_eq!(p.page, 2);
        assert_eq!(p.page_size, 50);
    }

    #[test]
    fn test_page_empty() {
        let p = Pagination::new(3, 10);
        let page: Page<String> = Page::empty(p);
        assert!(page.items.is_empty());
        assert_eq!(page.total, 0);
        assert_eq!(page.page, 3);
        assert_eq!(page.page_size, 10);
    }

    #[test]
    fn test_page_new() {
        let page = Page::new(vec![1, 2, 3], 100, 1, 3);
        assert_eq!(page.items, vec![1, 2, 3]);
        assert_eq!(page.total, 100);
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 3);
    }
}
