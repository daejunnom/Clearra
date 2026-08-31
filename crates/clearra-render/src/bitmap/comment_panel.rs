const MAX_COMMENT_CODE_POINTS: usize = 160;
const MAX_COMMENT_LINES: usize = 3;
const MIN_COMMENT_WIDTH: u32 = 80;
const PANEL_PADDING: u32 = 4;
const LINE_HEIGHT: u32 = 14;
const PANEL_BACKGROUND: [u8; 4] = [38, 50, 46, 255];
const PANEL_SEPARATOR: [u8; 4] = [103, 116, 111, 255];
const TEXT_COLOR: [u8; 4] = [255, 255, 255, 255];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommentPanelLayout {
    pub(crate) width: u32,
    pub(crate) height: u32,
    lines_by_page: Vec<Vec<String>>,
}

impl CommentPanelLayout {
    pub(crate) fn prepare(comments: &[String], board_pixel_width: u32) -> Option<Self> {
        let normalized = comments
            .iter()
            .map(|comment| normalize_comment(comment))
            .collect::<Vec<_>>();
        if normalized.iter().all(String::is_empty) {
            return None;
        }

        let width = board_pixel_width.max(MIN_COMMENT_WIDTH);
        let maximum_text_width = width.saturating_sub(PANEL_PADDING * 2);
        let lines_by_page = normalized
            .iter()
            .map(|comment| wrap_comment(comment, maximum_text_width))
            .collect::<Vec<_>>();
        let line_count = lines_by_page.iter().map(Vec::len).max().unwrap_or(0).max(1) as u32;
        Some(Self {
            width,
            height: 1 + PANEL_PADDING * 2 + line_count * LINE_HEIGHT,
            lines_by_page,
        })
    }

    pub(crate) fn paint(
        &self,
        rgba: &mut [u8],
        pixel_width: u32,
        panel_top: u32,
        page_index: usize,
    ) {
        fill_rectangle(
            rgba,
            pixel_width,
            0,
            panel_top,
            self.width,
            self.height,
            PANEL_BACKGROUND,
        );
        fill_rectangle(
            rgba,
            pixel_width,
            0,
            panel_top,
            self.width,
            1,
            PANEL_SEPARATOR,
        );
        let Some(lines) = self.lines_by_page.get(page_index) else {
            return;
        };
        for (index, line) in lines.iter().enumerate() {
            paint_comment_line(
                rgba,
                pixel_width,
                PANEL_PADDING,
                panel_top + PANEL_PADDING + 1 + index as u32 * LINE_HEIGHT,
                line,
            );
        }
    }
}

fn normalize_comment(value: &str) -> String {
    let mut safe = Vec::with_capacity(MAX_COMMENT_CODE_POINTS);
    let mut horizontal_space = false;
    for character in compose_modern_hangul(value) {
        if character == '\r' || character == '\n' {
            while safe.last() == Some(&' ') {
                safe.pop();
            }
            if !safe.is_empty() && safe.last() != Some(&'\n') {
                safe.push('\n');
            }
            horizontal_space = false;
            continue;
        }
        if character.is_whitespace() {
            if !safe.is_empty() && safe.last() != Some(&'\n') {
                horizontal_space = true;
            }
            continue;
        }
        if character.is_control() || is_disallowed_format(character) {
            continue;
        }
        if horizontal_space {
            safe.push(' ');
        }
        safe.push(character);
        horizontal_space = false;
        if safe.len() > MAX_COMMENT_CODE_POINTS {
            safe.truncate(MAX_COMMENT_CODE_POINTS - 1);
            safe.push('…');
            break;
        }
    }
    while matches!(safe.last(), Some(' ' | '\n')) {
        safe.pop();
    }
    safe.into_iter().collect()
}

fn compose_modern_hangul(value: &str) -> Vec<char> {
    const L_BASE: u32 = 0x1100;
    const V_BASE: u32 = 0x1161;
    const T_BASE: u32 = 0x11a7;
    const S_BASE: u32 = 0xac00;
    const V_COUNT: u32 = 21;
    const T_COUNT: u32 = 28;
    const N_COUNT: u32 = V_COUNT * T_COUNT;

    let source = value.chars().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        let leading = source[index] as u32;
        let Some(vowel) = source.get(index + 1).map(|character| *character as u32) else {
            output.push(source[index]);
            break;
        };
        if !(L_BASE..L_BASE + 19).contains(&leading) || !(V_BASE..V_BASE + V_COUNT).contains(&vowel)
        {
            output.push(source[index]);
            index += 1;
            continue;
        }
        let mut syllable = S_BASE + (leading - L_BASE) * N_COUNT + (vowel - V_BASE) * T_COUNT;
        index += 2;
        if let Some(trailing) = source.get(index).map(|character| *character as u32) {
            if (T_BASE + 1..T_BASE + T_COUNT).contains(&trailing) {
                syllable += trailing - T_BASE;
                index += 1;
            }
        }
        output.push(char::from_u32(syllable).expect("modern Hangul syllable is valid Unicode"));
    }
    output
}

const fn is_disallowed_format(character: char) -> bool {
    matches!(
        character as u32,
        0x00ad
            | 0x061c
            | 0x180e
            | 0x200b..=0x200f
            | 0x202a..=0x202e
            | 0x2060..=0x206f
            | 0xfeff
    )
}

fn wrap_comment(comment: &str, maximum_width: u32) -> Vec<String> {
    if comment.is_empty() {
        return Vec::new();
    }
    let source_lines = comment.split('\n').collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut truncated = false;
    for (source_index, source_line) in source_lines.iter().enumerate() {
        let characters = source_line.chars().collect::<Vec<_>>();
        if characters.is_empty() {
            if !output.is_empty() {
                output.push(String::new());
            }
            if output.len() >= MAX_COMMENT_LINES {
                truncated = source_index + 1 < source_lines.len();
                break;
            }
            continue;
        }
        let mut cursor = 0;
        while cursor < characters.len() {
            let mut end = cursor;
            let mut width = 0;
            let mut last_space = None;
            while end < characters.len() {
                let character = characters[end];
                let next_width = width + glyph_advance(character);
                if end > cursor && next_width > maximum_width {
                    break;
                }
                width = next_width;
                if character == ' ' {
                    last_space = Some(end);
                }
                end += 1;
            }
            if end < characters.len() {
                if let Some(space) = last_space.filter(|space| *space >= cursor) {
                    end = space;
                }
            }
            if end == cursor {
                end += 1;
            }
            let line = characters[cursor..end]
                .iter()
                .collect::<String>()
                .trim_end()
                .to_owned();
            if !line.is_empty() {
                output.push(line);
            }
            cursor = end;
            while characters.get(cursor) == Some(&' ') {
                cursor += 1;
            }
            if output.len() >= MAX_COMMENT_LINES {
                truncated = cursor < characters.len() || source_index + 1 < source_lines.len();
                break;
            }
        }
        if output.len() >= MAX_COMMENT_LINES {
            break;
        }
    }
    if truncated {
        if let Some(last) = output.last_mut() {
            append_ellipsis(last, maximum_width);
        }
    }
    output
}

fn append_ellipsis(value: &mut String, maximum_width: u32) {
    let mut characters = value.chars().collect::<Vec<_>>();
    while !characters.is_empty()
        && measure_text(characters.iter().copied().chain(core::iter::once('…'))) > maximum_width
    {
        characters.pop();
    }
    *value = characters
        .into_iter()
        .collect::<String>()
        .trim_end()
        .to_owned();
    value.push('…');
}

fn measure_text(characters: impl Iterator<Item = char>) -> u32 {
    characters.map(glyph_advance).sum()
}

const fn glyph_advance(character: char) -> u32 {
    if is_hangul_syllable(character) {
        13
    } else {
        6
    }
}

fn paint_comment_line(rgba: &mut [u8], width: u32, mut x: u32, y: u32, value: &str) {
    for character in value.chars() {
        if is_hangul_syllable(character) {
            paint_hangul_syllable(rgba, width, x, y, character);
        } else {
            paint_ascii_glyph(rgba, width, x, y, character);
        }
        x += glyph_advance(character);
    }
}

fn paint_ascii_glyph(rgba: &mut [u8], width: u32, x: u32, y: u32, character: char) {
    paint_bitmap(rgba, width, x, y, 5, 7, 5, ascii_glyph(character));
}

fn paint_hangul_syllable(rgba: &mut [u8], width: u32, x: u32, y: u32, character: char) {
    let syllable = character as u32 - 0xac00;
    let initial = (syllable / 588) as usize;
    let vowel = ((syllable % 588) / 28) as usize;
    let final_index = (syllable % 28) as usize;
    let has_final = final_index != 0;
    if matches!(vowel, 0..=7 | 20) {
        let upper_height = if has_final { 8 } else { 12 };
        paint_consonant(rgba, width, x, y, 6, upper_height, INITIALS[initial]);
        paint_bitmap(rgba, width, x + 6, y, 6, upper_height, 5, VOWELS[vowel]);
    } else if matches!(vowel, 8 | 12 | 13 | 17 | 18) {
        let initial_height = if has_final { 5 } else { 6 };
        let vowel_height = if has_final { 3 } else { 6 };
        paint_consonant(rgba, width, x + 1, y, 10, initial_height, INITIALS[initial]);
        paint_bitmap(
            rgba,
            width,
            x + 1,
            y + initial_height,
            10,
            vowel_height,
            5,
            VOWELS[vowel],
        );
    } else {
        let upper_height = if has_final { 8 } else { 12 };
        paint_consonant(rgba, width, x, y, 6, upper_height.min(6), INITIALS[initial]);
        paint_bitmap(rgba, width, x + 5, y, 7, upper_height, 5, VOWELS[vowel]);
    }
    if !has_final {
        return;
    }
    let (first, second) = FINALS[final_index];
    if let Some(second) = second {
        paint_consonant(rgba, width, x + 1, y + 9, 5, 3, first);
        paint_consonant(rgba, width, x + 6, y + 9, 5, 3, second);
    } else {
        paint_consonant(rgba, width, x + 1, y + 9, 10, 3, first);
    }
}

fn paint_consonant(
    rgba: &mut [u8],
    width: u32,
    x: u32,
    y: u32,
    box_width: u32,
    box_height: u32,
    consonant: u8,
) {
    if consonant & 0x80 == 0 {
        paint_bitmap(
            rgba,
            width,
            x,
            y,
            box_width,
            box_height,
            5,
            CONSONANTS[usize::from(consonant)],
        );
        return;
    }
    let consonant = consonant & 0x7f;
    let left_width = (box_width / 2).max(1);
    paint_bitmap(
        rgba,
        width,
        x,
        y,
        left_width,
        box_height,
        5,
        CONSONANTS[usize::from(consonant)],
    );
    paint_bitmap(
        rgba,
        width,
        x + left_width,
        y,
        (box_width - left_width).max(1),
        box_height,
        5,
        CONSONANTS[usize::from(consonant)],
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_bitmap(
    rgba: &mut [u8],
    output_width: u32,
    x: u32,
    y: u32,
    target_width: u32,
    target_height: u32,
    source_width: u32,
    source: &[u16],
) {
    let source_height = source.len() as u32;
    for target_y in 0..target_height {
        let source_y = if target_height == 1 {
            0
        } else {
            (target_y * (source_height - 1) + (target_height - 1) / 2) / (target_height - 1)
        };
        for target_x in 0..target_width {
            let source_x = if target_width == 1 {
                0
            } else {
                (target_x * (source_width - 1) + (target_width - 1) / 2) / (target_width - 1)
            };
            let mask = 1_u16 << (source_width - 1 - source_x);
            if source[source_y as usize] & mask == 0 {
                continue;
            }
            set_pixel(rgba, output_width, x + target_x, y + target_y, TEXT_COLOR);
        }
    }
}

fn fill_rectangle(
    rgba: &mut [u8],
    width: u32,
    x: u32,
    y: u32,
    rectangle_width: u32,
    rectangle_height: u32,
    color: [u8; 4],
) {
    for row in 0..rectangle_height {
        for column in 0..rectangle_width {
            set_pixel(rgba, width, x + column, y + row, color);
        }
    }
}

fn set_pixel(rgba: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 4]) {
    let Some(index) = (u64::from(y) * u64::from(width) + u64::from(x))
        .checked_mul(4)
        .and_then(|index| usize::try_from(index).ok())
    else {
        return;
    };
    if let Some(pixel) = rgba.get_mut(index..index + 4) {
        pixel.copy_from_slice(&color);
    }
}

const fn is_hangul_syllable(character: char) -> bool {
    matches!(character as u32, 0xac00..=0xd7a3)
}

const G: u8 = 0;
const N: u8 = 1;
const D: u8 = 2;
const R: u8 = 3;
const M: u8 = 4;
const B: u8 = 5;
const S: u8 = 6;
const NG: u8 = 7;
const J: u8 = 8;
const C: u8 = 9;
const K: u8 = 10;
const T: u8 = 11;
const P: u8 = 12;
const H: u8 = 13;
const DOUBLE: u8 = 0x80;

const INITIALS: [u8; 19] = [
    G,
    DOUBLE | G,
    N,
    D,
    DOUBLE | D,
    R,
    M,
    B,
    DOUBLE | B,
    S,
    DOUBLE | S,
    NG,
    J,
    DOUBLE | J,
    C,
    K,
    T,
    P,
    H,
];
const FINALS: [(u8, Option<u8>); 28] = [
    (G, None),
    (G, None),
    (DOUBLE | G, None),
    (G, Some(S)),
    (N, None),
    (N, Some(J)),
    (N, Some(H)),
    (D, None),
    (R, None),
    (R, Some(G)),
    (R, Some(M)),
    (R, Some(B)),
    (R, Some(S)),
    (R, Some(T)),
    (R, Some(P)),
    (R, Some(H)),
    (M, None),
    (B, None),
    (B, Some(S)),
    (S, None),
    (DOUBLE | S, None),
    (NG, None),
    (J, None),
    (C, None),
    (K, None),
    (T, None),
    (P, None),
    (H, None),
];

const CONSONANTS: [&[u16]; 14] = [
    &[0b11111, 0b00001, 0b00001, 0b00001, 0b00001],
    &[0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
    &[0b11111, 0b10001, 0b10001, 0b10001, 0b11111],
    &[0b11111, 0b00001, 0b11111, 0b10000, 0b11111],
    &[0b11111, 0b10001, 0b10001, 0b10001, 0b11111],
    &[0b10001, 0b10001, 0b11111, 0b10001, 0b11111],
    &[0b00100, 0b01010, 0b10001, 0b00000, 0b00000],
    &[0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
    &[0b11111, 0b00100, 0b01010, 0b10001, 0b00000],
    &[0b00100, 0b11111, 0b00100, 0b01010, 0b10001],
    &[0b11111, 0b00001, 0b11111, 0b00001, 0b00001],
    &[0b11111, 0b00000, 0b11111, 0b00000, 0b11111],
    &[0b10001, 0b11111, 0b10001, 0b11111, 0b10001],
    &[0b00100, 0b11111, 0b01110, 0b10001, 0b01110],
];

const VOWELS: [&[u16]; 21] = [
    &[0b00100, 0b00100, 0b00111, 0b00100, 0b00100],
    &[0b00101, 0b00101, 0b00111, 0b00101, 0b00101],
    &[0b00100, 0b00111, 0b00100, 0b00111, 0b00100],
    &[0b00101, 0b00111, 0b00101, 0b00111, 0b00101],
    &[0b00100, 0b00100, 0b11100, 0b00100, 0b00100],
    &[0b00101, 0b00101, 0b11101, 0b00101, 0b00101],
    &[0b00100, 0b11100, 0b00100, 0b11100, 0b00100],
    &[0b00101, 0b11101, 0b00101, 0b11101, 0b00101],
    &[0b00100, 0b00100, 0b11111, 0b00000, 0b00000],
    &[0b00101, 0b00101, 0b11111, 0b00100, 0b00100],
    &[0b00101, 0b00111, 0b11111, 0b00101, 0b00101],
    &[0b00101, 0b00101, 0b11111, 0b00001, 0b00001],
    &[0b01010, 0b01010, 0b11111, 0b00000, 0b00000],
    &[0b00000, 0b00000, 0b11111, 0b00100, 0b00100],
    &[0b00101, 0b00101, 0b11111, 0b00100, 0b00100],
    &[0b00101, 0b00111, 0b11111, 0b00101, 0b00101],
    &[0b00001, 0b00001, 0b11111, 0b00101, 0b00101],
    &[0b00000, 0b00000, 0b11111, 0b01010, 0b01010],
    &[0b00000, 0b00000, 0b11111, 0b00000, 0b00000],
    &[0b00001, 0b00001, 0b11111, 0b00001, 0b00001],
    &[0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
];

const REPLACEMENT: &[u16] = &[
    0b11111, 0b10001, 0b10101, 0b10001, 0b10101, 0b10001, 0b11111,
];

fn ascii_glyph(character: char) -> &'static [u16] {
    match character.to_ascii_uppercase() {
        ' ' => &[0, 0, 0, 0, 0, 0, 0],
        '!' => &[0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        '#' => &[0b01010, 0b11111, 0b01010, 0b01010, 0b11111, 0b01010, 0],
        '%' => &[0b11001, 0b11010, 0b00100, 0b01000, 0b10110, 0b00110, 0],
        '&' => &[
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
        '(' => &[
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => &[
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '+' => &[0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        ',' => &[0, 0, 0, 0, 0b00100, 0b00100, 0b01000],
        '-' => &[0, 0, 0, 0b11111, 0, 0, 0],
        '.' => &[0, 0, 0, 0, 0, 0b00100, 0b00100],
        '/' => &[0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0, 0],
        '0' => &[
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => &[
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => &[
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => &[
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => &[
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => &[
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => &[
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => &[
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => &[
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => &[
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ':' => &[0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0],
        '<' => &[
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ],
        '=' => &[0, 0b11111, 0, 0b11111, 0, 0, 0],
        '>' => &[
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ],
        '?' => &[0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '@' => &[
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ],
        'A' => &[
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => &[
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => &[
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => &[
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => &[
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => &[
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => &[
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => &[
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => &[
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => &[
            0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110,
        ],
        'K' => &[
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => &[
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => &[
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => &[
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => &[
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => &[
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => &[
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => &[
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => &[
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => &[
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => &[
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => &[
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => &[
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => &[
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => &[
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => &[
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '_' => &[0, 0, 0, 0, 0, 0, 0b11111],
        '…' => &[0, 0, 0, 0, 0, 0b10101, 0b10101],
        _ => REPLACEMENT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_comments_create_no_panel_and_untrusted_text_is_bounded() {
        assert_eq!(
            CommentPanelLayout::prepare(&[" \n\t\u{202e}".to_owned()], 80),
            None
        );
        let panel = CommentPanelLayout::prepare(
            &[format!("<b>@everyone</b> 한글 주석 {}", "😀".repeat(200))],
            80,
        )
        .expect("comment panel");
        assert!(panel.height <= 51);
        assert_eq!(panel.lines_by_page[0].len(), 3);
        assert!(panel.lines_by_page[0].last().unwrap().ends_with('…'));
        assert_eq!(normalize_comment("한글 주석"), "한글 주석");
        assert_eq!(
            normalize_comment(&"A".repeat(MAX_COMMENT_CODE_POINTS)),
            "A".repeat(MAX_COMMENT_CODE_POINTS)
        );
    }

    #[test]
    fn hangul_comment_paints_white_pixels_inside_the_panel() {
        let panel = CommentPanelLayout::prepare(&["한글 주석".to_owned()], 80).unwrap();
        let mut rgba = vec![0; (panel.width * panel.height * 4) as usize];
        panel.paint(&mut rgba, panel.width, 0, 0);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel == TEXT_COLOR));
    }
}
