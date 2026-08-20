use super::*;
use crate::normalize::FumenNormalizer;

#[test]
fn fumen_page_roundtrip() {
    let trace = FumenLikeTrace::new(vec![
        "kind=pc\nlines=2\ncomment=keep me".to_owned(),
        "kind=normalized-solution\ninitial_board_mask=0x1\nfinal_board_mask=0x0\npiece_sequence=IO\noperation_sequence=I:0:0:0|O:0:4:0".to_owned(),
    ]);

    assert_eq!(
        FumenTransformContract::page_roundtrip(&trace).expect("roundtrip"),
        trace
    );
}

#[test]
fn fumen_mirror_roundtrip() {
    let trace = FumenLikeTrace::new(vec![
        "kind=normalized-solution\ninitial_board_mask=0x1\nfinal_board_mask=0x0\npiece_sequence=TI\noperation_sequence=T:0:4:0|I:1:2:1\nmirror_policy=none".to_owned(),
    ]);
    let mirrored = FumenTransformContract::field_mirror(&trace);
    let encoded = FumenLikeWriter::write(&mirrored).expect("mirrored fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded mirrored fumen");
    let normalized = FumenNormalizer::normalize_trace(&decoded);
    let key = normalized
        .solution_keys()
        .iter()
        .next()
        .expect("solution key");

    assert_eq!(key.mirror_policy(), "field-mirror");
    assert!(decoded.pages()[0].contains("mirror_policy=field-mirror"));
}

#[test]
fn fumen_transform_contract_combines_splits_grayouts_comments_and_shifts_pages() {
    let left = vec!["kind=page-a\nfree comment".to_owned()];
    let right = vec!["kind=page-b\ngrayout_normalized=false".to_owned()];
    let combined = FumenTransformContract::combine(&[left.clone(), right.clone()]);

    assert_eq!(combined.pages().len(), 2);
    assert_eq!(FumenTransformContract::split(&combined).len(), 2);

    let grayout = FumenTransformContract::grayout(&combined);
    assert!(grayout.pages()[1].contains("grayout_normalized=true"));

    let stripped = FumenTransformContract::remove_comments(&combined);
    assert!(!stripped.pages()[0].contains("free comment"));
    assert_eq!(
        FumenTransformContract::preserve_comments(&combined),
        combined
    );

    let shifted = FumenTransformContract::page_shift(&combined, 1);
    assert!(shifted.pages()[0].contains("kind=page-b"));
}

#[test]
fn fumen_to_build_template_adapter_validates_input() {
    let trace = FumenLikeTrace::new(vec![
        "kind=build-template\ntemplate_id=t-spin-setup\nslot_count=4\nmirror_policy=none"
            .to_owned(),
    ]);
    let encoded = FumenLikeWriter::write(&trace).expect("build template fumen");
    let draft = FumenToBuildTemplateAdapter::build_template_from_fumen(&encoded).expect("draft");

    assert_eq!(draft.template_id(), "t-spin-setup");
    assert_eq!(draft.slot_count(), 4);
    assert_eq!(draft.mirror_policy(), "none");

    let invalid = FumenLikeTrace::new(vec!["kind=replay-trace".to_owned()]);
    assert_eq!(
        FumenToBuildTemplateAdapter::build_template_from_trace(&invalid),
        Err(BuildTemplateError::UnsupportedPageKind {
            kind: "replay-trace".to_owned()
        })
    );
}
