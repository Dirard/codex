use super::HeadTailBuffer;
use super::LastBytes;

use pretty_assertions::assert_eq;

#[test]
fn keeps_prefix_and_suffix_when_over_budget() {
    let mut buf = HeadTailBuffer::<10>::default();

    buf.push_chunk(b"0123456789");
    assert_eq!(buf.omitted_bytes(), 0);

    // Exceeds max by 2; we should keep head+tail and omit the middle.
    buf.push_chunk(b"ab");
    assert!(buf.omitted_bytes() > 0);

    let rendered = String::from_utf8_lossy(&buf.to_bytes()).to_string();
    assert!(rendered.starts_with("01234"));
    assert!(rendered.ends_with("89ab"));
    assert_eq!(
        String::from_utf8_lossy(&buf.to_bytes_with_omission_marker()),
        "01234\n... 2 bytes omitted ...\n789ab"
    );
}

#[test]
fn max_bytes_zero_drops_everything() {
    let mut buf = HeadTailBuffer::<0>::default();
    buf.push_chunk(b"abc");

    assert_eq!(buf.retained_bytes(), 0);
    assert_eq!(buf.omitted_bytes(), 3);
    assert_eq!(buf.to_bytes(), b"");
}

#[test]
fn head_budget_zero_keeps_only_last_byte_in_tail() {
    let mut buf = HeadTailBuffer::<1>::default();
    buf.push_chunk(b"abc");

    assert_eq!(buf.retained_bytes(), 1);
    assert_eq!(buf.omitted_bytes(), 2);
    assert_eq!(buf.to_bytes(), b"c");
}

#[test]
fn chunk_larger_than_tail_budget_keeps_only_tail_end() {
    let mut buf = HeadTailBuffer::<10>::default();
    buf.push_chunk(b"0123456789");

    // Tail budget is 5 bytes. This chunk should replace the tail and keep only its last 5 bytes.
    buf.push_chunk(b"ABCDEFGHIJK");

    let out = String::from_utf8_lossy(&buf.to_bytes()).to_string();
    assert!(out.starts_with("01234"));
    assert!(out.ends_with("GHIJK"));
    assert!(buf.omitted_bytes() > 0);
}

#[test]
fn fills_head_then_tail_across_multiple_chunks() {
    let mut buf = HeadTailBuffer::<10>::default();

    // Fill the 5-byte head budget across multiple chunks.
    buf.push_chunk(b"01");
    buf.push_chunk(b"234");
    assert_eq!(buf.to_bytes(), b"01234");

    // Then fill the 5-byte tail budget.
    buf.push_chunk(b"567");
    buf.push_chunk(b"89");
    assert_eq!(buf.to_bytes(), b"0123456789");
    assert_eq!(buf.omitted_bytes(), 0);

    // One more byte causes the tail to drop its oldest byte.
    buf.push_chunk(b"a");
    assert_eq!(buf.to_bytes(), b"012346789a");
    assert_eq!(buf.omitted_bytes(), 1);
}

#[test]
fn empty_and_tiny_chunks_have_bounded_metadata() {
    let mut buf = HeadTailBuffer::<10>::default();

    for byte in b"0123456789ab" {
        buf.push_chunk(&[]);
        buf.push_chunk(&[*byte]);
    }

    assert_eq!(buf.retained_bytes(), 10);
    assert_eq!(buf.omitted_bytes(), 2);
}

#[test]
fn last_bytes_reads_recent_output_without_draining() {
    let last = |bytes: &[u8], readable_earlier, lost| LastBytes {
        bytes: bytes.to_vec(),
        readable_earlier,
        lost,
    };
    let mut buf = HeadTailBuffer::<10>::default();
    buf.push_chunk(b"0123");
    assert_eq!(buf.last_bytes(2), last(b"23", 2, 0));
    assert_eq!(buf.last_bytes(10), last(b"0123", 0, 0));

    // Head "01234" and tail "56789" are contiguous while nothing was omitted.
    buf.push_chunk(b"456789");
    assert_eq!(buf.last_bytes(7), last(b"3456789", 3, 0));

    // After an omission the head is no longer contiguous with the tail. Fork (R2.9): the
    // dropped bytes are lost; only the retained bytes before the tail are still readable.
    buf.push_chunk(b"ab");
    assert_eq!(buf.omitted_bytes(), 2);
    assert_eq!(buf.last_bytes(3), last(b"9ab", 7, 2));
    assert_eq!(buf.last_bytes(8), last(b"789ab", 5, 2));
    assert_eq!(buf.retained_bytes(), 10);
}
