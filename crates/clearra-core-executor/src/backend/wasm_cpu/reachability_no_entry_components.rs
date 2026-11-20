//! Optional bounded no-entry proof over horizontal translation components.

use super::{ReachabilityTemplate, INVALID_STATE_INDEX, INVALID_STATE_MASK};

const MAX_RUNS: usize = 16;
const MAX_BOARD_WIDTH: u8 = 10;
const MAX_PREDECESSORS_CHECKED: usize = 4096;
// The exact template compiler uses [down, left, right] for these slots.
const LEFT: usize = 1;
const RIGHT: usize = 2;

#[cfg(test)]
thread_local! {
    static TEST_MODE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

pub(super) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = TEST_MODE.with(std::cell::Cell::get) {
        return enabled;
    }
    #[cfg(feature = "local-search-ab")]
    {
        static MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            std::env::var("CLEARRA_NO_ENTRY_COMPONENTS").is_ok_and(|value| value == "1")
        })
    }
    #[cfg(not(feature = "local-search-ab"))]
    {
        false
    }
}

#[cfg(test)]
pub(super) fn with_mode<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_MODE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(TEST_MODE.with(|mode| mode.replace(Some(enabled))));
    run()
}

#[derive(Clone, Copy)]
struct HorizontalRun {
    first: u16,
    last: u16,
}

impl HorizontalRun {
    fn contains(self, pose: u16) -> bool {
        (self.first..=self.last).contains(&pose)
    }
}

/// Enlarge the previous small pose budget without running a board-wide
/// primitive search. At fixed rotation and y, contract a run only across
/// exact, bidirectional template edges with both endpoints clear at minimum.
/// Iterate the complete optimistic reverse boundary of every retained run,
/// including all geometrically retained kicks and the real sky-seed test.
///
/// In the optimistic graph every board with additional occupancy only removes
/// vertices; it never adds an edge that is not in this graph. A run is therefore
/// not asserted to survive on every superset. It is simply an exact quotient
/// at minimum for proving that no superset can enter this closure.
pub(super) fn proves_no_entry(
    template: &ReachabilityTemplate,
    minimum: u64,
    target: usize,
) -> bool {
    if !(1..=MAX_BOARD_WIDTH).contains(&template.width) {
        return false;
    }
    let Ok(target) = u16::try_from(target) else {
        return false;
    };
    if !pose_is_clear(template, minimum, target) {
        return false;
    }

    let width = u16::from(template.width);
    let root = horizontal_run(template, minimum, width, target);
    let mut runs = [root; MAX_RUNS];
    let mut run_count = 1;
    let mut cursor = 0;
    let mut predecessor_count = 0;
    while cursor < run_count {
        let run = runs[cursor];
        cursor += 1;
        for pose in run.first..=run.last {
            let pose = usize::from(pose);
            if template.is_sky_seed(pose) {
                return false;
            }
            let begin = template.reverse_rotation_offsets[pose] as usize;
            let end = template.reverse_rotation_offsets[pose + 1] as usize;
            let rotations = template.reverse_rotation_sources[begin..end]
                .iter()
                .map(|source| source.source);
            for source in template.reverse_translation_sources[pose]
                .into_iter()
                .chain(rotations)
            {
                predecessor_count += 1;
                if predecessor_count > MAX_PREDECESSORS_CHECKED {
                    return false;
                }
                if !pose_is_clear(template, minimum, source)
                    || runs[..run_count].iter().any(|run| run.contains(source))
                {
                    continue;
                }
                if template.is_sky_seed(usize::from(source)) {
                    return false;
                }
                let Some(next) = runs.get_mut(run_count) else {
                    return false;
                };
                *next = horizontal_run(template, minimum, width, source);
                run_count += 1;
            }
        }
    }
    true
}

fn pose_is_clear(template: &ReachabilityTemplate, minimum: u64, pose: u16) -> bool {
    pose != INVALID_STATE_INDEX
        && template
            .state_masks
            .get(usize::from(pose))
            .is_some_and(|&mask| mask != INVALID_STATE_MASK && minimum & mask == 0)
}

fn horizontal_run(
    template: &ReachabilityTemplate,
    minimum: u64,
    width: u16,
    pose: u16,
) -> HorizontalRun {
    let mut first = pose;
    while first % width != 0 {
        let next = first - 1;
        if !pose_is_clear(template, minimum, next)
            || template.translation_targets[usize::from(first)][LEFT] != next
            || template.translation_targets[usize::from(next)][RIGHT] != first
        {
            break;
        }
        first = next;
    }
    let mut last = pose;
    while last % width != width - 1 {
        let next = last + 1;
        if !pose_is_clear(template, minimum, next)
            || template.translation_targets[usize::from(last)][RIGHT] != next
            || template.translation_targets[usize::from(next)][LEFT] != last
        {
            break;
        }
        last = next;
    }
    HorizontalRun { first, last }
}
