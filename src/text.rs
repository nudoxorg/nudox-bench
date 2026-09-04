//! Byte offsets that are safe to cut a string at.

/// The largest index at or below `index` that lies on a character boundary.
///
/// # Why this exists
///
/// Every size limit in this repo is a byte count — `MAX_OUTPUT_BYTES`, a
/// transcript cap, a preview width — and every string it is applied to is
/// real source text or a tool's rendering of it. Source is full of
/// multi-byte characters: an accented name in a comment, a `→` in a doc
/// string, a `…` in a message. Cutting at a raw byte offset splits one of
/// those roughly one time in three, and both `&s[..n]` and `String::truncate`
/// panic when it happens — taking down a whole run at whatever scenario
/// happened to produce a long enough output.
///
/// `str::floor_char_boundary` does exactly this but is still unstable, so it
/// is spelled out here.
#[must_use]
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut end = index;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::floor_char_boundary;

    #[test]
    fn an_index_inside_a_character_walks_back_to_its_start() {
        let s = "xxxxxxxxxé tail"; // 'é' occupies bytes 9..11
        assert_eq!(floor_char_boundary(s, 10), 9);
        assert_eq!(floor_char_boundary(s, 11), 11);
        assert_eq!(floor_char_boundary(s, 9), 9);
    }

    #[test]
    fn an_index_past_the_end_is_the_end() {
        assert_eq!(floor_char_boundary("abc", 99), 3);
        assert_eq!(floor_char_boundary("", 5), 0);
    }

    #[test]
    fn the_result_is_always_safe_to_slice_at() {
        // The property that matters: whatever comes back can be cut at.
        let s = "café → naïve ☃ 日本語";
        for index in 0..=s.len() + 4 {
            let cut = floor_char_boundary(s, index);
            assert!(s.is_char_boundary(cut), "index {index} produced {cut}");
            let _ = &s[..cut];
            let mut owned = s.to_owned();
            owned.truncate(cut);
        }
    }

    #[test]
    fn ascii_is_never_moved() {
        let s = "plain ascii only";
        for index in 0..=s.len() {
            assert_eq!(floor_char_boundary(s, index), index);
        }
    }
}
