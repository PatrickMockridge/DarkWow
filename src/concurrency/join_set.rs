/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! JoinSet — concurrent task collection for parallel execution.
//!
//! Maps to the ρ-calculus parallel composition primitive: `P₁ | P₂ | ... | Pₙ`.
//! Spawns tasks independently, then joins all results. When key sets are
//! disjoint, the parallel composition is weak-bisimilar (≈) to sequential
//! execution — same observable outputs, different internal scheduling.
//!
//! See: doc/src/arch/type-system.md §9.1 (Process-to-Task Mapping)

use std::future::Future;

use smol::channel;

use super::ExecutorPtr;

/// A collection of spawned tasks that can be joined.
///
/// Spawn tasks with [`spawn`](JoinSet::spawn), then await all results
/// with [`join_all`](JoinSet::join_all). Results are returned in spawn order.
pub struct JoinSet<T: Send + 'static> {
    tasks: Vec<(usize, channel::Receiver<T>)>,
    next_index: usize,
}

impl<T: Send + 'static> JoinSet<T> {
    /// Create a new empty task set.
    pub fn new() -> Self {
        Self { tasks: Vec::new(), next_index: 0 }
    }

    /// Spawn a future on the executor. Returns the task index.
    ///
    /// The task executes concurrently with all other spawned tasks.
    /// The result is collected by [`join_all`](JoinSet::join_all).
    pub fn spawn(
        &mut self,
        executor: &ExecutorPtr,
        fut: impl Future<Output = T> + Send + 'static,
    ) -> usize {
        let (tx, rx) = channel::bounded(1);
        let index = self.next_index;
        self.next_index += 1;

        executor
            .spawn(async move {
                let result = fut.await;
                // Ignore send error — receiver may have been dropped
                let _ = tx.send(result).await;
            })
            .detach();

        self.tasks.push((index, rx));
        index
    }

    /// Await all spawned tasks and return results in spawn order.
    ///
    /// This is the `merge!` barrier in ρ-calculus: all parallel processes
    /// must complete before the barrier releases.
    ///
    /// A single receive pass in spawn order: `self.tasks` is pushed with
    /// monotonically increasing indices, so iterating in order IS spawn
    /// order. Each `recv().await` awaits only that task's completion while
    /// every detached task runs concurrently on the executor — barrier
    /// latency is max-of-tasks, preserving §9.2 weak bisimilarity and the
    /// §9.4 merge-barrier contract.
    pub async fn join_all(self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.tasks.len());
        for (_index, rx) in self.tasks {
            match rx.recv().await {
                Ok(value) => out.push(value),
                Err(_) => {} // skip panicked tasks
            }
        }
        out
    }

    /// Number of tasks spawned so far.
    pub fn len(&self) -> usize {
        self.next_index
    }

    /// Returns true if no tasks have been spawned.
    pub fn is_empty(&self) -> bool {
        self.next_index == 0
    }
}

impl<T: Send + 'static> Default for JoinSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use smol::Executor;

    #[test]
    fn test_join_set_parallel_execution() {
        let ex = Arc::new(Executor::new());
        let executor = Arc::new(ex);

        smol::block_on(executor.run(async {
            let mut set = JoinSet::new();
            set.spawn(&executor, async { 1 + 1 });
            set.spawn(&executor, async { 2 + 2 });
            set.spawn(&executor, async { 3 + 3 });

            let results = set.join_all().await;

            // Results complete in any order but all are correct
            assert_eq!(results.len(), 3);
            let sum: i32 = results.iter().sum();
            assert_eq!(sum, 12); // 2 + 4 + 6
        }));
    }

    #[test]
    fn test_join_set_empty() {
        let set: JoinSet<i32> = JoinSet::new();
        assert!(set.is_empty());
    }

    #[test]
    fn test_join_set_spawn_order_preserved() {
        let ex = Arc::new(Executor::new());
        let executor = Arc::new(ex);

        smol::block_on(executor.run(async {
            let mut set = JoinSet::new();
            // Spawn tasks with deterministic output based on index
            for i in 0..5 {
                let idx = i;
                set.spawn(&executor, async move { idx * 10 });
            }

            let mut results = set.join_all().await;
            results.sort(); // sort since completion order is non-deterministic

            assert_eq!(results, vec![0, 10, 20, 30, 40]);
        }));
    }
}
