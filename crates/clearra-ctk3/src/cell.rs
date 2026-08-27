use crate::big_nat::{combination_count, combination_rank, combination_unrank, BigNat};
use crate::bitstream::{BitReader, BitWriter};
use crate::Ctk3CodecError;

pub(crate) fn write_best_cell_encoding(
    writer: &mut BitWriter,
    codes: &[u8],
    previous: Option<&[u8]>,
    mode_width: usize,
    include_multiset: bool,
) -> Result<(), Ctk3CodecError> {
    let mut candidates = vec![
        palette_encoding(codes, mode_width)?,
        run_length_encoding(codes, mode_width)?,
        occupancy_encoding(codes, mode_width)?,
    ];
    if let Some(previous) = previous {
        candidates.push(delta_encoding(codes, previous, mode_width)?);
        if mode_width >= 4 {
            candidates.push(change_mask_encoding(codes, previous, mode_width)?);
            candidates.push(delta_run_encoding(codes, previous, mode_width)?);
            if let Some(candidate) = single_color_delta_encoding(codes, previous, mode_width)? {
                candidates.push(candidate);
            }
            candidates.push(combinatorial_delta_encoding(codes, previous, mode_width)?);
        }
    }
    if include_multiset {
        if codes.iter().all(|code| *code == 0) {
            candidates.push(single_mode_encoding(7, mode_width)?);
        }
        if previous.is_some_and(|prior| arrays_equal_fitted(codes, prior)) {
            candidates.push(single_mode_encoding(6, mode_width)?);
        }
        candidates.push(multiset_encoding(codes, mode_width)?);
        if let Some(candidate) = tetromino_color_encoding(codes, mode_width)? {
            candidates.push(candidate);
        }
    }
    append_shortest(writer, candidates);
    Ok(())
}

pub(crate) fn write_best_predicted_cell_encoding(
    writer: &mut BitWriter,
    codes: &[u8],
    predictor: &[u8],
    mode_width: usize,
) -> Result<(), Ctk3CodecError> {
    let mut candidates = vec![
        delta_encoding(codes, predictor, mode_width)?,
        change_mask_encoding(codes, predictor, mode_width)?,
        delta_run_encoding(codes, predictor, mode_width)?,
        combinatorial_delta_encoding(codes, predictor, mode_width)?,
    ];
    if codes == predictor {
        candidates.push(single_mode_encoding(6, mode_width)?);
    }
    if let Some(candidate) = single_color_delta_encoding(codes, predictor, mode_width)? {
        candidates.push(candidate);
    }
    append_shortest(writer, candidates);
    Ok(())
}

fn append_shortest(writer: &mut BitWriter, candidates: Vec<BitWriter>) {
    let mut candidates = candidates.into_iter();
    let mut shortest = candidates
        .next()
        .expect("CTK3 always has an encoding candidate");
    for candidate in candidates {
        if candidate.bit_len < shortest.bit_len {
            shortest = candidate;
        }
    }
    writer.append(&shortest);
}

fn single_mode_encoding(mode: u32, mode_width: usize) -> Result<BitWriter, Ctk3CodecError> {
    let mut writer = BitWriter::default();
    writer.write_bits(mode, mode_width)?;
    Ok(writer)
}

fn palette_encoding(codes: &[u8], mode_width: usize) -> Result<BitWriter, Ctk3CodecError> {
    let mut writer = BitWriter::default();
    writer.write_bits(0, mode_width)?;
    let mut palette = unique_codes(codes);
    if palette.is_empty() {
        palette.push(0);
    }
    writer.write_bits(color_mask(&palette), 9)?;
    let width = bits_for_choices(palette.len());
    for code in codes {
        let index = palette
            .iter()
            .position(|candidate| candidate == code)
            .expect("palette contains every source color");
        writer.write_bits(index as u32, width)?;
    }
    Ok(writer)
}

fn run_length_encoding(codes: &[u8], mode_width: usize) -> Result<BitWriter, Ctk3CodecError> {
    let mut writer = BitWriter::default();
    writer.write_bits(1, mode_width)?;
    let mut runs: Vec<(u8, usize)> = Vec::new();
    for code in codes {
        if let Some((last_code, length)) = runs.last_mut().filter(|(last, _)| last == code) {
            let _ = last_code;
            *length += 1;
        } else {
            runs.push((*code, 1));
        }
    }
    writer.write_var_uint(runs.len() as u64)?;
    for (code, length) in runs {
        writer.write_bits(u32::from(code), 4)?;
        writer.write_var_uint((length - 1) as u64)?;
    }
    Ok(writer)
}

fn delta_encoding(
    codes: &[u8],
    previous: &[u8],
    mode_width: usize,
) -> Result<BitWriter, Ctk3CodecError> {
    let changes = changed_cells(codes, previous);
    let mut writer = BitWriter::default();
    writer.write_bits(2, mode_width)?;
    writer.write_var_uint(changes.len() as u64)?;
    let mut prior = None;
    for (index, code) in changes {
        let gap = prior.map_or(index, |prior| index - prior - 1);
        writer.write_var_uint(gap as u64)?;
        writer.write_bits(u32::from(code), 4)?;
        prior = Some(index);
    }
    Ok(writer)
}

fn change_mask_encoding(
    codes: &[u8],
    previous: &[u8],
    mode_width: usize,
) -> Result<BitWriter, Ctk3CodecError> {
    let changed = codes
        .iter()
        .enumerate()
        .map(|(index, code)| u8::from(*code != previous.get(index).copied().unwrap_or(0)))
        .collect::<Vec<_>>();
    let changed_codes = codes
        .iter()
        .zip(&changed)
        .filter_map(|(code, changed)| (*changed != 0).then_some(*code))
        .collect::<Vec<_>>();
    let palette = unique_codes(&changed_codes);
    let mut writer = BitWriter::default();
    writer.write_bits(8, mode_width)?;
    write_occupancy_mask(&mut writer, &changed)?;
    writer.write_bits(color_mask(&palette), 9)?;
    let width = bits_for_choices(palette.len());
    for (code, changed) in codes.iter().zip(changed) {
        if changed != 0 {
            let index = palette
                .iter()
                .position(|candidate| candidate == code)
                .expect("delta palette contains every changed color");
            writer.write_bits(index as u32, width)?;
        }
    }
    Ok(writer)
}

fn delta_run_encoding(
    codes: &[u8],
    previous: &[u8],
    mode_width: usize,
) -> Result<BitWriter, Ctk3CodecError> {
    let changes = changed_cells(codes, previous);
    let mut runs: Vec<(usize, Vec<u8>)> = Vec::new();
    for (index, code) in changes {
        if let Some((start, run_codes)) = runs
            .last_mut()
            .filter(|(start, values)| *start + values.len() == index)
        {
            let _ = start;
            run_codes.push(code);
        } else {
            runs.push((index, vec![code]));
        }
    }
    let flattened = runs
        .iter()
        .flat_map(|(_, values)| values.iter().copied())
        .collect::<Vec<_>>();
    let palette = unique_codes(&flattened);
    let mut writer = BitWriter::default();
    writer.write_bits(9, mode_width)?;
    writer.write_bits(color_mask(&palette), 9)?;
    writer.write_var_uint(runs.len() as u64)?;
    let mut previous_end = 0usize;
    for (start, values) in &runs {
        writer.write_var_uint((start - previous_end) as u64)?;
        writer.write_var_uint((values.len() - 1) as u64)?;
        previous_end = start + values.len();
    }
    let width = bits_for_choices(palette.len());
    for (_, values) in runs {
        for code in values {
            let index = palette
                .iter()
                .position(|candidate| *candidate == code)
                .expect("run palette contains every changed color");
            writer.write_bits(index as u32, width)?;
        }
    }
    Ok(writer)
}

fn single_color_delta_encoding(
    codes: &[u8],
    previous: &[u8],
    mode_width: usize,
) -> Result<Option<BitWriter>, Ctk3CodecError> {
    let changed = codes
        .iter()
        .enumerate()
        .map(|(index, code)| u8::from(*code != previous.get(index).copied().unwrap_or(0)))
        .collect::<Vec<_>>();
    let colors = unique_codes(
        &codes
            .iter()
            .zip(&changed)
            .filter_map(|(code, changed)| (*changed != 0).then_some(*code))
            .collect::<Vec<_>>(),
    );
    if colors.len() != 1 {
        return Ok(None);
    }
    let mut writer = BitWriter::default();
    writer.write_bits(10, mode_width)?;
    writer.write_bits(u32::from(colors[0]), 4)?;
    write_occupancy_mask(&mut writer, &changed)?;
    Ok(Some(writer))
}

fn combinatorial_delta_encoding(
    codes: &[u8],
    previous: &[u8],
    mode_width: usize,
) -> Result<BitWriter, Ctk3CodecError> {
    let positions = codes
        .iter()
        .enumerate()
        .filter_map(|(index, code)| {
            (*code != previous.get(index).copied().unwrap_or(0)).then_some(index)
        })
        .collect::<Vec<_>>();
    let mut writer = BitWriter::default();
    writer.write_bits(11, mode_width)?;
    writer.write_var_uint(positions.len() as u64)?;
    let choice_count = combination_count(codes.len(), positions.len());
    writer.write_big_bits(
        &combination_rank(&positions),
        bits_for_big_choices(&choice_count),
    )?;
    let palette = unique_codes(
        &positions
            .iter()
            .map(|index| codes[*index])
            .collect::<Vec<_>>(),
    );
    writer.write_bits(color_mask(&palette), 9)?;
    let width = bits_for_choices(palette.len());
    for position in positions {
        let code = codes[position];
        let index = palette
            .iter()
            .position(|candidate| *candidate == code)
            .expect("combinatorial palette contains every changed color");
        writer.write_bits(index as u32, width)?;
    }
    Ok(writer)
}

fn occupancy_encoding(codes: &[u8], mode_width: usize) -> Result<BitWriter, Ctk3CodecError> {
    let occupied = codes
        .iter()
        .map(|code| u8::from(*code != 0))
        .collect::<Vec<_>>();
    let non_empty = codes
        .iter()
        .copied()
        .filter(|code| *code != 0)
        .collect::<Vec<_>>();
    let palette = unique_codes(&non_empty);
    let mut writer = BitWriter::default();
    writer.write_bits(3, mode_width)?;
    write_occupancy_mask(&mut writer, &occupied)?;
    writer.write_bits(color_mask(&palette) >> 1, 8)?;
    let width = bits_for_choices(palette.len());
    for code in codes.iter().copied().filter(|code| *code != 0) {
        let index = palette
            .iter()
            .position(|candidate| *candidate == code)
            .expect("occupancy palette contains every occupied color");
        writer.write_bits(index as u32, width)?;
    }
    Ok(writer)
}

fn write_occupancy_mask(writer: &mut BitWriter, occupied: &[u8]) -> Result<(), Ctk3CodecError> {
    let mut raw = BitWriter::default();
    raw.write_bit(false);
    for value in occupied {
        raw.write_bit(*value != 0);
    }

    let mut runs = BitWriter::default();
    runs.write_bit(true);
    if let Some(first) = occupied.first() {
        runs.write_bit(*first != 0);
        let mut run_length = 1usize;
        for index in 1..=occupied.len() {
            if index < occupied.len() && occupied[index] == occupied[index - 1] {
                run_length += 1;
            } else {
                runs.write_var_uint((run_length - 1) as u64)?;
                run_length = 1;
            }
        }
    }
    writer.append(if raw.bit_len <= runs.bit_len {
        &raw
    } else {
        &runs
    });
    Ok(())
}

fn multiset_encoding(codes: &[u8], mode_width: usize) -> Result<BitWriter, Ctk3CodecError> {
    let mut palette = unique_codes(codes);
    if palette.is_empty() {
        palette.push(0);
    }
    let mut writer = BitWriter::default();
    writer.write_bits(4, mode_width)?;
    writer.write_bits(color_mask(&palette), 9)?;
    let mut remaining = (0..codes.len()).collect::<Vec<_>>();
    for code in palette.iter().take(palette.len().saturating_sub(1)) {
        let positions = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, source)| (codes[*source] == *code).then_some(index))
            .collect::<Vec<_>>();
        writer.write_var_uint(positions.len() as u64)?;
        let choices = combination_count(remaining.len(), positions.len());
        writer.write_big_bits(
            &combination_rank(&positions),
            bits_for_big_choices(&choices),
        )?;
        remove_relative_positions(&mut remaining, &positions);
    }
    Ok(writer)
}

fn tetromino_color_encoding(
    codes: &[u8],
    mode_width: usize,
) -> Result<Option<BitWriter>, Ctk3CodecError> {
    let mut counts = [0usize; 9];
    for code in codes {
        if *code == 1 {
            return Ok(None);
        }
        if *code != 0 {
            counts[*code as usize] += 1;
        }
    }
    let palette = (2u8..=8)
        .filter(|code| counts[*code as usize] != 0)
        .collect::<Vec<_>>();
    if palette.is_empty() || palette.iter().any(|code| counts[*code as usize] != 4) {
        return Ok(None);
    }
    let occupied = codes
        .iter()
        .map(|code| u8::from(*code != 0))
        .collect::<Vec<_>>();
    let mut writer = BitWriter::default();
    writer.write_bits(5, mode_width)?;
    write_occupancy_mask(&mut writer, &occupied)?;
    writer.write_bits(color_mask(&palette) >> 2, 7)?;
    let mut remaining = codes
        .iter()
        .enumerate()
        .filter_map(|(index, code)| (*code != 0).then_some(index))
        .collect::<Vec<_>>();
    for code in palette.iter().take(palette.len().saturating_sub(1)) {
        let positions = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, source)| (codes[*source] == *code).then_some(index))
            .collect::<Vec<_>>();
        let choices = combination_count(remaining.len(), 4);
        writer.write_big_bits(
            &combination_rank(&positions),
            bits_for_big_choices(&choices),
        )?;
        remove_relative_positions(&mut remaining, &positions);
    }
    Ok(Some(writer))
}

fn remove_relative_positions(values: &mut Vec<usize>, positions: &[usize]) {
    let mut cursor = 0usize;
    values.retain(|_| {
        let keep = positions.binary_search(&cursor).is_err();
        cursor += 1;
        keep
    });
}

fn changed_cells(codes: &[u8], previous: &[u8]) -> Vec<(usize, u8)> {
    codes
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, code)| *code != previous.get(*index).copied().unwrap_or(0))
        .collect()
}

fn arrays_equal_fitted(codes: &[u8], previous: &[u8]) -> bool {
    codes
        .iter()
        .enumerate()
        .all(|(index, code)| *code == previous.get(index).copied().unwrap_or(0))
}

pub(crate) fn read_cell_encoding(
    reader: &mut BitReader<'_>,
    cell_count: usize,
    previous: Option<&[u8]>,
    mode_width: usize,
) -> Result<Vec<u8>, Ctk3CodecError> {
    let mode = reader.read_bits(mode_width)?;
    if mode == 0 {
        let palette = palette_from_mask(reader.read_bits(9)?, 0);
        if palette.is_empty() {
            return Err(Ctk3CodecError::invalid("palette is empty"));
        }
        let width = bits_for_choices(palette.len());
        return (0..cell_count)
            .map(|_| {
                let index = reader.read_bits(width)? as usize;
                palette
                    .get(index)
                    .copied()
                    .ok_or_else(|| Ctk3CodecError::invalid("palette index is invalid"))
            })
            .collect();
    }
    if mode == 1 {
        let run_count = reader.read_var_uint()? as usize;
        if run_count > cell_count {
            return Err(Ctk3CodecError::invalid("color run count is invalid"));
        }
        let mut codes = Vec::with_capacity(cell_count);
        for _ in 0..run_count {
            let code = reader.read_bits(4)? as u8;
            assert_color_code(code)?;
            let length = reader.read_var_uint()? as usize + 1;
            if length > cell_count - codes.len() {
                return Err(Ctk3CodecError::invalid("color run exceeds the field"));
            }
            codes.extend(core::iter::repeat_n(code, length));
        }
        if codes.len() != cell_count {
            return Err(Ctk3CodecError::invalid("color runs do not fill the field"));
        }
        return Ok(codes);
    }
    if mode == 2 {
        let mut codes = fitted_previous(previous, cell_count)?;
        let change_count = reader.read_var_uint()? as usize;
        if change_count > cell_count {
            return Err(Ctk3CodecError::invalid("delta count exceeds the field"));
        }
        let mut previous_index: Option<usize> = None;
        for _ in 0..change_count {
            let gap = reader.read_var_uint()? as usize;
            let index =
                previous_index.map_or(gap, |prior| prior.saturating_add(gap).saturating_add(1));
            let code = reader.read_bits(4)? as u8;
            assert_color_code(code)?;
            if index >= cell_count || previous_index.is_some_and(|prior| index <= prior) {
                return Err(Ctk3CodecError::invalid("delta cell index is invalid"));
            }
            codes[index] = code;
            previous_index = Some(index);
        }
        return Ok(codes);
    }
    if mode == 4 && mode_width >= 3 {
        return read_multiset_encoding(reader, cell_count);
    }
    if mode == 5 && mode_width >= 3 {
        return read_tetromino_color_encoding(reader, cell_count);
    }
    if mode == 6 && mode_width >= 3 {
        return fitted_previous(previous, cell_count);
    }
    if mode == 7 && mode_width >= 3 {
        return Ok(vec![0; cell_count]);
    }
    if mode == 8 && mode_width >= 4 {
        let mut codes = fitted_previous(previous, cell_count)?;
        let changed = read_occupancy_mask(reader, cell_count)?;
        let palette = palette_from_mask(reader.read_bits(9)?, 0);
        if changed.iter().any(|value| *value != 0) && palette.is_empty() {
            return Err(Ctk3CodecError::invalid("delta palette is empty"));
        }
        let width = bits_for_choices(palette.len());
        for index in 0..cell_count {
            if changed[index] != 0 {
                codes[index] = read_palette_code(reader, &palette, width, "delta palette index")?;
            }
        }
        return Ok(codes);
    }
    if mode == 9 && mode_width >= 4 {
        let mut codes = fitted_previous(previous, cell_count)?;
        let palette = palette_from_mask(reader.read_bits(9)?, 0);
        let run_count = reader.read_var_uint()? as usize;
        if run_count > cell_count {
            return Err(Ctk3CodecError::invalid("delta run count is invalid"));
        }
        let mut runs = Vec::with_capacity(run_count);
        let mut previous_end = 0usize;
        let mut change_count = 0usize;
        for _ in 0..run_count {
            let start = previous_end
                .checked_add(reader.read_var_uint()? as usize)
                .ok_or(Ctk3CodecError::IntegerOverflow)?;
            let length = reader.read_var_uint()? as usize + 1;
            if start < previous_end || start > cell_count || length > cell_count - start {
                return Err(Ctk3CodecError::invalid("delta run is invalid"));
            }
            change_count += length;
            runs.push((start, length));
            previous_end = start + length;
        }
        if change_count > 0 && palette.is_empty() {
            return Err(Ctk3CodecError::invalid("delta palette is empty"));
        }
        let width = bits_for_choices(palette.len());
        for (start, length) in runs {
            for offset in 0..length {
                codes[start + offset] =
                    read_palette_code(reader, &palette, width, "delta palette index")?;
            }
        }
        return Ok(codes);
    }
    if mode == 10 && mode_width >= 4 {
        let mut codes = fitted_previous(previous, cell_count)?;
        let code = reader.read_bits(4)? as u8;
        assert_color_code(code)?;
        let changed = read_occupancy_mask(reader, cell_count)?;
        for (target, changed) in codes.iter_mut().zip(changed) {
            if changed != 0 {
                *target = code;
            }
        }
        return Ok(codes);
    }
    if mode == 11 && mode_width >= 4 {
        let mut codes = fitted_previous(previous, cell_count)?;
        let count = reader.read_var_uint()? as usize;
        if count > cell_count {
            return Err(Ctk3CodecError::invalid("delta count exceeds the field"));
        }
        let choices = combination_count(cell_count, count);
        let rank = reader.read_big_bits(bits_for_big_choices(&choices))?;
        if rank >= choices {
            return Err(Ctk3CodecError::invalid("delta rank is invalid"));
        }
        let positions = combination_unrank(cell_count, count, &rank)
            .ok_or_else(|| Ctk3CodecError::invalid("delta rank is invalid"))?;
        let palette = palette_from_mask(reader.read_bits(9)?, 0);
        if count > 0 && palette.is_empty() {
            return Err(Ctk3CodecError::invalid("delta palette is empty"));
        }
        let width = bits_for_choices(palette.len());
        for position in positions {
            codes[position] = read_palette_code(reader, &palette, width, "delta palette index")?;
        }
        return Ok(codes);
    }
    if mode != 3 {
        return Err(Ctk3CodecError::invalid("field encoding mode is invalid"));
    }
    let occupied = read_occupancy_mask(reader, cell_count)?;
    let palette = palette_from_mask(reader.read_bits(8)? << 1, 1);
    if occupied.iter().any(|value| *value != 0) && palette.is_empty() {
        return Err(Ctk3CodecError::invalid("occupied color palette is empty"));
    }
    let width = bits_for_choices(palette.len());
    occupied
        .into_iter()
        .map(|occupied| {
            if occupied == 0 {
                Ok(0)
            } else {
                read_palette_code(reader, &palette, width, "occupied color index")
            }
        })
        .collect()
}

fn fitted_previous(previous: Option<&[u8]>, cell_count: usize) -> Result<Vec<u8>, Ctk3CodecError> {
    let previous =
        previous.ok_or_else(|| Ctk3CodecError::invalid("delta field has no predictor"))?;
    Ok((0..cell_count)
        .map(|index| previous.get(index).copied().unwrap_or(0))
        .collect())
}

fn read_occupancy_mask(
    reader: &mut BitReader<'_>,
    cell_count: usize,
) -> Result<Vec<u8>, Ctk3CodecError> {
    if !reader.read_bit()? {
        return (0..cell_count)
            .map(|_| Ok(u8::from(reader.read_bit()?)))
            .collect();
    }
    let mut occupied = Vec::with_capacity(cell_count);
    if cell_count > 0 {
        let mut value = u8::from(reader.read_bit()?);
        while occupied.len() < cell_count {
            let length = reader.read_var_uint()? as usize + 1;
            if length > cell_count - occupied.len() {
                return Err(Ctk3CodecError::invalid("occupancy run exceeds the field"));
            }
            occupied.extend(core::iter::repeat_n(value, length));
            value ^= 1;
        }
    }
    Ok(occupied)
}

fn read_multiset_encoding(
    reader: &mut BitReader<'_>,
    cell_count: usize,
) -> Result<Vec<u8>, Ctk3CodecError> {
    let palette = palette_from_mask(reader.read_bits(9)?, 0);
    if palette.is_empty() {
        return Err(Ctk3CodecError::invalid("multiset palette is empty"));
    }
    let mut codes = vec![u8::MAX; cell_count];
    let mut remaining = (0..cell_count).collect::<Vec<_>>();
    for code in palette.iter().take(palette.len().saturating_sub(1)) {
        let count = reader.read_var_uint()? as usize;
        if count > remaining.len() {
            return Err(Ctk3CodecError::invalid("multiset count exceeds the field"));
        }
        let choices = combination_count(remaining.len(), count);
        let rank = reader.read_big_bits(bits_for_big_choices(&choices))?;
        if rank >= choices {
            return Err(Ctk3CodecError::invalid("multiset rank is invalid"));
        }
        let positions = combination_unrank(remaining.len(), count, &rank)
            .ok_or_else(|| Ctk3CodecError::invalid("multiset rank is invalid"))?;
        for position in &positions {
            codes[remaining[*position]] = *code;
        }
        remove_relative_positions(&mut remaining, &positions);
    }
    for index in remaining {
        codes[index] = *palette.last().expect("non-empty palette");
    }
    Ok(codes)
}

fn read_tetromino_color_encoding(
    reader: &mut BitReader<'_>,
    cell_count: usize,
) -> Result<Vec<u8>, Ctk3CodecError> {
    let occupied = read_occupancy_mask(reader, cell_count)?;
    let palette = palette_from_mask(reader.read_bits(7)? << 2, 2);
    let occupied_count = occupied.iter().filter(|value| **value != 0).count();
    if palette.is_empty() || occupied_count != palette.len() * 4 {
        return Err(Ctk3CodecError::invalid("tetromino color field is invalid"));
    }
    let mut codes = vec![0; cell_count];
    let mut remaining = occupied
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value != 0).then_some(index))
        .collect::<Vec<_>>();
    for code in palette.iter().take(palette.len().saturating_sub(1)) {
        let choices = combination_count(remaining.len(), 4);
        let rank = reader.read_big_bits(bits_for_big_choices(&choices))?;
        if rank >= choices {
            return Err(Ctk3CodecError::invalid("tetromino color rank is invalid"));
        }
        let positions = combination_unrank(remaining.len(), 4, &rank)
            .ok_or_else(|| Ctk3CodecError::invalid("tetromino color rank is invalid"))?;
        for position in &positions {
            codes[remaining[*position]] = *code;
        }
        remove_relative_positions(&mut remaining, &positions);
    }
    for index in remaining {
        codes[index] = *palette.last().expect("non-empty palette");
    }
    Ok(codes)
}

fn read_palette_code(
    reader: &mut BitReader<'_>,
    palette: &[u8],
    width: usize,
    context: &'static str,
) -> Result<u8, Ctk3CodecError> {
    palette
        .get(reader.read_bits(width)? as usize)
        .copied()
        .ok_or_else(|| Ctk3CodecError::invalid(context))
}

fn unique_codes(codes: &[u8]) -> Vec<u8> {
    let mut present = [false; 9];
    for code in codes {
        present[*code as usize] = true;
    }
    present
        .iter()
        .enumerate()
        .filter_map(|(code, present)| present.then_some(code as u8))
        .collect()
}

fn color_mask(codes: &[u8]) -> u32 {
    codes.iter().fold(0, |mask, code| mask | (1 << code))
}

fn palette_from_mask(mask: u32, minimum: u8) -> Vec<u8> {
    (minimum..=8)
        .filter(|code| mask & (1 << code) != 0)
        .collect()
}

pub(crate) fn bits_for_choices(choice_count: usize) -> usize {
    if choice_count <= 1 {
        0
    } else {
        usize::BITS as usize - (choice_count - 1).leading_zeros() as usize
    }
}

fn bits_for_big_choices(choice_count: &BigNat) -> usize {
    if choice_count <= &BigNat::one() {
        0
    } else {
        choice_count.subtract_one().bit_len()
    }
}

fn assert_color_code(code: u8) -> Result<(), Ctk3CodecError> {
    if code <= 8 {
        Ok(())
    } else {
        Err(Ctk3CodecError::invalid("field color is invalid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cell_mode_round_trips_through_best_encoder() {
        let sources = [
            vec![],
            vec![0; 40],
            vec![2, 2, 2, 2, 0, 0, 0, 0, 4, 4, 4, 4],
            (0..180).map(|index| (index % 9) as u8).collect(),
        ];
        for source in sources {
            let mut writer = BitWriter::default();
            write_best_cell_encoding(&mut writer, &source, None, 4, true).expect("encode");
            let bytes = writer.into_bytes();
            let mut reader = BitReader::new(&bytes);
            assert_eq!(
                read_cell_encoding(&mut reader, source.len(), None, 4),
                Ok(source)
            );
        }
    }
}
