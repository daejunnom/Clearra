use super::*;

#[test]
fn rejects_empty_trace_pages() {
    assert_eq!(
        FumenLikeTrace::try_new(vec!["ok".to_owned(), String::new()]),
        Err(FumenLikeTraceError::EmptyPage { index: 1 })
    );
}

#[test]
fn accepts_separator_text_as_page_content() {
    let trace = FumenLikeTrace::try_new(vec!["a\n---\nb".to_owned()]).expect("trace");

    assert_eq!(trace.pages(), &["a\n---\nb".to_owned()]);
}

#[test]
fn rejects_carriage_return_newlines() {
    assert_eq!(
        FumenLikeTrace::try_new(vec!["a\r\nb".to_owned()]),
        Err(FumenLikeTraceError::CarriageReturn { index: 0 })
    );
}
