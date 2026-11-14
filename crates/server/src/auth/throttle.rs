//! Slowing down password and code guessing.

use std::time::Duration;

use tokio::sync::Mutex;

/// How long a failed attempt blocks the next one.
const PENALTY: Duration = Duration::from_secs(1);

/// Serializes secret checks and pauses after each failure, so guessing
/// runs at no more than one failed attempt per second, however many
/// requests arrive in parallel.
#[derive(Debug, Default)]
pub(crate) struct Throttle {
    gate: Mutex<()>,
}

impl Throttle {
    /// Runs `check`; when it returns `false`, holds the gate for the
    /// penalty before letting anyone else check.
    pub(crate) async fn check<F, E>(&self, check: F) -> Result<bool, E>
    where
        F: Future<Output = Result<bool, E>>,
    {
        let _gate = self.gate.lock().await;
        let passed = check.await?;
        if !passed {
            tokio::time::sleep(PENALTY).await;
        }
        Ok(passed)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn failures_are_serialized_with_a_penalty() {
        let throttle = Throttle::default();
        let started = tokio::time::Instant::now();
        let fail = || throttle.check(async { Ok::<_, Infallible>(false) });
        let (a, b) = tokio::join!(fail(), fail());
        assert_eq!((a, b), (Ok(false), Ok(false)));
        assert_eq!(started.elapsed(), PENALTY * 2);
    }

    #[tokio::test(start_paused = true)]
    async fn success_is_not_delayed() {
        let throttle = Throttle::default();
        let started = tokio::time::Instant::now();
        assert_eq!(
            throttle.check(async { Ok::<_, Infallible>(true) }).await,
            Ok(true)
        );
        assert_eq!(started.elapsed(), Duration::ZERO);
    }
}
