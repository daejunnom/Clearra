//! Local A/B representation shared by CPU and WebGPU proposers.
//! A round is simultaneous: every constraint reads the same input snapshot.
//! This is not an AtMost receipt and cannot close a product proof by itself.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchCoverMatrix {
    candidate_count: usize,
    offsets: Vec<u32>,
    candidates: Vec<u32>,
    demands: Vec<u32>,
}

impl BatchCoverMatrix {
    pub fn new(
        candidate_count: usize,
        constraints: &[(Vec<u32>, u32)],
    ) -> Result<Self, &'static str> {
        if candidate_count == 0 || candidate_count > 4096 || constraints.is_empty() {
            return Err("invalid batch matrix dimensions");
        }
        let mut offsets = vec![0];
        let mut candidates = Vec::new();
        let mut demands = Vec::new();
        for (support, demand) in constraints {
            if !matches!(demand, 1 | 2)
                || support.iter().any(|&v| v as usize >= candidate_count)
                || !support.windows(2).all(|w| w[0] < w[1])
            {
                return Err("invalid or duplicate batch constraint");
            }
            candidates.extend_from_slice(support);
            offsets.push(u32::try_from(candidates.len()).map_err(|_| "matrix size overflow")?);
            demands.push(*demand);
        }
        Ok(Self {
            candidate_count,
            offsets,
            candidates,
            demands,
        })
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_count
    }
    pub fn constraint_count(&self) -> usize {
        self.demands.len()
    }
    pub fn support(&self, row: usize) -> &[u32] {
        &self.candidates[self.offsets[row] as usize..self.offsets[row + 1] as usize]
    }
    pub fn demand(&self, row: usize) -> u32 {
        self.demands[row]
    }

    /// Offsets, then demands, then candidate IDs. Original candidate IDs remain intact.
    pub fn device_words(&self) -> Vec<u32> {
        let mut words = self.offsets.clone();
        words.extend_from_slice(&self.demands);
        words.extend_from_slice(&self.candidates);
        words
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedCoverStates {
    candidate_count: usize,
    state_count: usize,
    groups: usize,
    selected: Vec<u32>,
    excluded: Vec<u32>,
    limits: Vec<u32>,
}

impl PackedCoverStates {
    /// Validate the shared SoA wire representation without expanding assignments
    /// into per-state candidate lists and packing them a second time.
    pub fn from_device_words(
        candidate_count: usize,
        state_count: usize,
        mut words: Vec<u32>,
    ) -> Result<Self, &'static str> {
        if candidate_count == 0
            || candidate_count > 4096
            || state_count == 0
            || state_count > 262_144
        {
            return Err("invalid batch state dimensions");
        }
        let groups = state_count.div_ceil(32);
        let span = candidate_count * groups;
        if words.len() != 2 * span + state_count {
            return Err("invalid packed state length");
        }
        let limits = words.split_off(2 * span);
        let excluded = words.split_off(span);
        let selected = words;
        if limits.iter().any(|&limit| limit as usize > candidate_count)
            || selected.iter().zip(&excluded).any(|(t, f)| t & f != 0)
        {
            return Err("invalid packed assignment or limit");
        }
        if state_count % 32 != 0 {
            let padding = !((1_u32 << (state_count % 32)) - 1);
            for candidate in 0..candidate_count {
                let index = candidate * groups + groups - 1;
                if (selected[index] | excluded[index]) & padding != 0 {
                    return Err("nonzero packed state padding");
                }
            }
        }
        Ok(Self {
            candidate_count,
            state_count,
            groups,
            selected,
            excluded,
            limits,
        })
    }

    pub fn new(
        candidate_count: usize,
        states: &[(Vec<u32>, Vec<u32>, u32)],
    ) -> Result<Self, &'static str> {
        if candidate_count == 0
            || candidate_count > 4096
            || states.is_empty()
            || states.len() > 262_144
        {
            return Err("invalid batch state dimensions");
        }
        let groups = states.len().div_ceil(32);
        let mut result = Self {
            candidate_count,
            state_count: states.len(),
            groups,
            selected: vec![0; candidate_count * groups],
            excluded: vec![0; candidate_count * groups],
            limits: Vec::with_capacity(states.len()),
        };
        for (state, (selected, excluded, limit)) in states.iter().enumerate() {
            if *limit as usize > candidate_count {
                return Err("invalid row limit");
            }
            for (list, output) in [
                (selected, &mut result.selected),
                (excluded, &mut result.excluded),
            ] {
                if !list.windows(2).all(|w| w[0] < w[1])
                    || list.iter().any(|&v| v as usize >= candidate_count)
                {
                    return Err("invalid or duplicate candidate assignment");
                }
                for &candidate in list {
                    output[candidate as usize * groups + state / 32] |= 1 << (state % 32);
                }
            }
            result.limits.push(*limit);
        }
        if result
            .selected
            .iter()
            .zip(&result.excluded)
            .any(|(t, f)| t & f != 0)
        {
            return Err("contradictory input assignment");
        }
        Ok(result)
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_count
    }
    pub fn state_count(&self) -> usize {
        self.state_count
    }
    pub fn groups(&self) -> usize {
        self.groups
    }
    pub fn selected(&self) -> &[u32] {
        &self.selected
    }
    pub fn excluded(&self) -> &[u32] {
        &self.excluded
    }
    pub fn limits(&self) -> &[u32] {
        &self.limits
    }
    pub fn active_mask(&self, group: usize) -> u32 {
        let remaining = self.state_count.saturating_sub(group * 32);
        if remaining >= 32 {
            u32::MAX
        } else {
            (1_u32 << remaining) - 1
        }
    }
    pub fn device_words(&self) -> Vec<u32> {
        let mut words = self.selected.clone();
        words.extend_from_slice(&self.excluded);
        words.extend_from_slice(&self.limits);
        words
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverPropagationRound {
    /// Device layout: selected, excluded, then one conflict bit per input state.
    pub words: Vec<u32>,
}

pub fn propagate_round(
    matrix: &BatchCoverMatrix,
    input: &PackedCoverStates,
    bit_sliced: bool,
) -> Result<CoverPropagationRound, &'static str> {
    if matrix.candidate_count != input.candidate_count {
        return Err("batch identity mismatch");
    }
    let span = input.selected.len();
    let mut output = input.device_words();
    output.truncate(span * 2);
    output.resize(span * 2 + input.groups, 0);
    if bit_sliced {
        for row in 0..matrix.constraint_count() {
            let support = matrix.support(row);
            for group in 0..input.groups {
                let (mut one, mut two, mut three, mut selected_one, mut selected_two) =
                    (0, 0, 0, 0, 0);
                for &candidate in support {
                    let i = candidate as usize * input.groups + group;
                    let available = !input.excluded[i];
                    three |= two & available;
                    two |= one & available;
                    one |= available;
                    selected_two |= selected_one & input.selected[i];
                    selected_one |= input.selected[i];
                }
                let (possible, satisfied, exact) = if matrix.demand(row) == 1 {
                    (one, selected_one, one & !two)
                } else {
                    (two, selected_two, two & !three)
                };
                let active = input.active_mask(group);
                output[span * 2 + group] |= !possible & active;
                let force = exact & !satisfied & active;
                if force != 0 {
                    for &candidate in support {
                        let i = candidate as usize * input.groups + group;
                        output[i] |= force & !input.excluded[i];
                    }
                }
            }
        }
    } else {
        for state in 0..input.state_count {
            let (group, bit) = (state / 32, 1_u32 << (state % 32));
            for row in 0..matrix.constraint_count() {
                let (mut selected, mut available) = (0_u32, 0_u32);
                for &candidate in matrix.support(row) {
                    let i = candidate as usize * input.groups + group;
                    selected += u32::from(input.selected[i] & bit != 0);
                    available += u32::from(input.excluded[i] & bit == 0);
                }
                let demand = matrix.demand(row);
                if available < demand {
                    output[span * 2 + group] |= bit;
                }
                if selected < demand && available == demand {
                    for &candidate in matrix.support(row) {
                        let i = candidate as usize * input.groups + group;
                        output[i] |= bit & !input.excluded[i];
                    }
                }
            }
        }
    }
    // A separate seal reads the completed simultaneous positive propagation.
    // No mutable selected-count increments, and no conflicting GPU read/write phase.
    for state in 0..input.state_count {
        let (group, bit) = (state / 32, 1_u32 << (state % 32));
        let mut count = 0_u32;
        for candidate in 0..input.candidate_count {
            let i = candidate * input.groups + group;
            count += u32::from(output[i] & bit != 0);
            if output[i] & input.excluded[i] & bit != 0 {
                output[span * 2 + group] |= bit;
            }
        }
        if count > input.limits[state] {
            output[span * 2 + group] |= bit;
        }
        if count == input.limits[state] {
            for candidate in 0..input.candidate_count {
                let i = candidate * input.groups + group;
                output[span + i] |= bit & !output[i];
            }
        }
    }
    Ok(CoverPropagationRound { words: output })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propagation_preserves_every_feasible_completion() {
        let constraints = vec![(vec![0, 1], 1), (vec![1, 2, 3], 2), (vec![0, 3, 4], 1)];
        let matrix = BatchCoverMatrix::new(5, &constraints).unwrap();
        let mut states = Vec::new();
        for encoded in 0..3_usize.pow(5) {
            let mut value = encoded;
            let (mut selected, mut excluded) = (Vec::new(), Vec::new());
            for candidate in 0..5 {
                match value % 3 {
                    1 => selected.push(candidate),
                    2 => excluded.push(candidate),
                    _ => {}
                }
                value /= 3;
            }
            for limit in 0..=5 {
                states.push((selected.clone(), excluded.clone(), limit));
            }
        }
        let packed = PackedCoverStates::new(5, &states).unwrap();
        let flat = propagate_round(&matrix, &packed, false).unwrap();
        assert_eq!(flat, propagate_round(&matrix, &packed, true).unwrap());
        let span = 5 * packed.groups();
        for (state, (selected, excluded, limit)) in states.iter().enumerate() {
            let (group, bit) = (state / 32, 1 << (state % 32));
            for solution in 0..32_u32 {
                if solution.count_ones() > *limit
                    || selected.iter().any(|v| solution & (1 << v) == 0)
                    || excluded.iter().any(|v| solution & (1 << v) != 0)
                    || constraints.iter().any(|(support, demand)| {
                        support
                            .iter()
                            .filter(|v| solution & (1 << **v) != 0)
                            .count()
                            < *demand as usize
                    })
                {
                    continue;
                }
                assert_eq!(
                    flat.words[2 * span + group] & bit,
                    0,
                    "feasible state {state}"
                );
                for candidate in 0..5 {
                    let index = candidate * packed.groups() + group;
                    if flat.words[index] & bit != 0 {
                        assert_ne!(solution & (1 << candidate), 0);
                    }
                    if flat.words[span + index] & bit != 0 {
                        assert_eq!(solution & (1 << candidate), 0);
                    }
                }
            }
        }
        let last_mask = packed.active_mask(packed.groups() - 1);
        assert_eq!(
            flat.words.last().unwrap() & !last_mask,
            0,
            "padding cannot become a conflict"
        );
    }

    #[test]
    fn assignment_identity_and_word_boundaries_are_preserved() {
        for count in [1, 31, 32, 33, 63, 64, 65, 246] {
            let matrix = BatchCoverMatrix::new(count, &[(vec![(count - 1) as u32], 1)]).unwrap();
            for state_count in [1, 31, 32, 33, 63, 64, 65] {
                let states = (0..state_count)
                    .map(|s| {
                        (
                            Vec::new(),
                            if s % 3 == 0 {
                                vec![(count - 1) as u32]
                            } else {
                                vec![]
                            },
                            1,
                        )
                    })
                    .collect::<Vec<_>>();
                let packed = PackedCoverStates::new(count, &states).unwrap();
                assert_eq!(
                    packed,
                    PackedCoverStates::from_device_words(count, state_count, packed.device_words())
                        .unwrap()
                );
                let result = propagate_round(&matrix, &packed, true).unwrap();
                assert_eq!(result, propagate_round(&matrix, &packed, false).unwrap());
                let span = count * packed.groups();
                for state in 0..state_count {
                    let bit = 1 << (state % 32);
                    assert_eq!(
                        result.words[2 * span + state / 32] & bit != 0,
                        state % 3 == 0
                    );
                }
            }
        }
        assert!(PackedCoverStates::new(2, &[(vec![1], vec![1], 1)]).is_err());
        assert!(BatchCoverMatrix::new(2, &[(vec![0, 0], 2)]).is_err());
        assert!(PackedCoverStates::from_device_words(1, 1, vec![2, 0, 1]).is_err());
        assert!(PackedCoverStates::from_device_words(1, 1, vec![1, 1, 1]).is_err());
        assert!(PackedCoverStates::from_device_words(1, 1, vec![0, 0, 2]).is_err());
    }
}
