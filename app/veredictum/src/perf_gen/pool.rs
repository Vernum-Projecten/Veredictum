// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! The update-target pool: a lease hands one target to one holder at a time,
//! so no two in-flight updates address the same versioned object.
//!
//! Two concurrent updates of one object race on its version, and the loser's
//! `412` would be the instrument's own doing rather than the server's. No
//! openEHR specification governs this: our own design.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

/// A pool of update targets leased out one holder at a time.
#[derive(Debug)]
pub struct LeasePool<T> {
    free: Arc<Mutex<VecDeque<T>>>,
}

impl<T> Clone for LeasePool<T> {
    fn clone(&self) -> Self {
        Self {
            free: Arc::clone(&self.free),
        }
    }
}

impl<T> LeasePool<T> {
    /// Builds a pool over `targets`.
    #[must_use]
    pub fn new(targets: Vec<T>) -> Self {
        Self {
            free: Arc::new(Mutex::new(targets.into())),
        }
    }

    /// Leases the target that has been free longest, or `None` when every
    /// target is leased.
    ///
    /// The target returns to the back of the pool when the lease drops, so
    /// successive leases rotate through the pool.
    #[must_use]
    pub fn lease(&self) -> Option<Lease<T>> {
        let target = self
            .free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()?;
        Some(Lease {
            target: Some(target),
            home: Arc::clone(&self.free),
        })
    }

    /// How many targets are free right now.
    #[must_use]
    pub fn free(&self) -> usize {
        self.free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// One leased target, returned to its pool on drop.
#[derive(Debug)]
pub struct Lease<T> {
    target: Option<T>,
    home: Arc<Mutex<VecDeque<T>>>,
}

impl<T> Lease<T> {
    /// The leased target.
    #[must_use]
    pub fn target(&self) -> Option<&T> {
        self.target.as_ref()
    }
}

impl<T> Drop for Lease<T> {
    fn drop(&mut self) {
        if let Some(target) = self.target.take() {
            self.home
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LeasePool;

    #[test]
    fn a_target_is_never_leased_twice_at_once() {
        let pool = LeasePool::new(vec!["a", "b"]);
        let first = pool.lease().unwrap();
        let second = pool.lease().unwrap();
        assert_ne!(first.target(), second.target());
        assert!(pool.lease().is_none(), "both targets are out");
        drop(first);
        let third = pool.lease().unwrap();
        assert_eq!(third.target(), Some(&"a"));
        assert_eq!(pool.free(), 0);
        drop(second);
        drop(third);
        assert_eq!(pool.free(), 2);
    }
}
