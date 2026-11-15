//! The time the site gives a build: recognizing it, or testing it, stops
//! once its budget runs out, with [`Rule::TookTooLong`], rather than
//! holding a core while someone waits. Wherever a test lets its circuit
//! settle, between one case and the next, it checks the clock, so one
//! check covers every test, however it runs. A budget is set around a
//! whole piece of work with [`within`], on the thread that does it, and
//! carried into the threads a timing spreads over, see [`carried`];
//! where none is set, as in the test suite, nothing stops. The site sets
//! one limit for the whole process, [`set_limit`], which recognizing a
//! build and evaluating one each run within, see [`limited`].

use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use anyhow::Result;

use super::{Rule, Untested};

/// What recognizing a build, testing it, or measuring one that passes may
/// each take on the site.
pub const SECONDS: u64 = 5;

/// The limit [`limited`] work runs within, in milliseconds; 0 for none.
static LIMIT: AtomicU64 = AtomicU64::new(0);

/// Makes all [`limited`] work in this process stop after `limit`, as the
/// site does once, as it starts.
pub fn set_limit(limit: Duration) {
    let millis = u64::try_from(limit.as_millis()).unwrap_or(u64::MAX);
    LIMIT.store(millis.max(1), Ordering::Relaxed);
}

/// Runs `work`, a whole piece of it such as recognizing a build, within
/// the limit [`set_limit`] set, if any, or what is left of the work it is
/// part of.
pub fn limited<T>(work: impl FnOnce() -> T) -> T {
    match LIMIT.load(Ordering::Relaxed) {
        0 => work(),
        millis => within(Duration::from_millis(millis), work),
    }
}

/// Whether `err` is that the work it ended ran out of time, as [`check`]
/// fails, so what it was measuring is unknown rather than wrong.
pub fn ran_out(err: &anyhow::Error) -> bool {
    err.downcast_ref::<Untested>()
        .is_some_and(|untested| untested.rule == Rule::TookTooLong)
}

/// When the work under way must stop, and the time it was given.
pub type Deadline = Option<(Instant, Duration)>;

thread_local! {
    static DEADLINE: Cell<Deadline> = const { Cell::new(None) };
}

/// Runs `work` with `limit` to run in, or less if work it is part of has
/// less left, as a test within a recognition does; the deadline before is
/// back once it returns.
pub fn within<T>(limit: Duration, work: impl FnOnce() -> T) -> T {
    let own = (Instant::now() + limit, limit);
    let outer = deadline();
    let sooner = match outer {
        Some(outer) if outer.0 <= own.0 => outer,
        _ => own,
    };
    carried(Some(sooner), work)
}

/// Runs `work` under `deadline`, as a thread a timing spreads over runs
/// under its parent's; the deadline before is back once it returns.
pub fn carried<T>(deadline: Deadline, work: impl FnOnce() -> T) -> T {
    let before = DEADLINE.with(|cell| cell.replace(deadline));
    let done = work();
    DEADLINE.with(|cell| cell.set(before));
    done
}

/// The deadline of the work under way on this thread, if any.
pub fn deadline() -> Deadline {
    DEADLINE.with(Cell::get)
}

/// Fails once the work under way has run past its deadline.
pub fn check() -> Result<()> {
    if let Some((at, limit)) = deadline()
        && Instant::now() >= at
    {
        untested!(
            Rule::TookTooLong;
            "It was still being simulated after {} seconds, the most the \
             site gives a build.",
            limit.as_secs_f32()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_stops_once_its_time_is_up_and_the_soonest_deadline_holds() {
        assert!(check().is_ok());
        within(Duration::ZERO, || {
            let err = check().unwrap_err();
            assert!(err.to_string().contains("still being simulated"));
            assert!(ran_out(&err));
            // Work within it has no more time than it.
            within(Duration::from_secs(10), || assert!(check().is_err()));
        });
        assert!(check().is_ok());
        within(Duration::from_secs(10), || {
            assert!(check().is_ok());
            let deadline = deadline();
            std::thread::spawn(move || {
                carried(deadline, || assert!(check().is_ok()));
            })
            .join()
            .unwrap();
        });
    }
}
