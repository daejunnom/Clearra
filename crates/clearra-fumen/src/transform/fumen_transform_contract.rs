use crate::{
    codec::{
        FumenLikeReadError, FumenLikeReader, FumenLikeTrace, FumenLikeWriteError, FumenLikeWriter,
    },
    transform::{
        CombineTransform, GrayoutTransform, MirrorTransform, PageShiftTransform, SplitTransform,
    },
};

/// Legacy Clearra metadata-page transform.
///
/// This type operates on `FumenLikeTrace` comment payloads and is not the
/// product authority for v115 field, color, flag, or operation transforms.
/// Product utility commands must use `ActualFumenDocumentTransform`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FumenTransformContract;

impl FumenTransformContract {
    pub fn page_roundtrip(trace: &FumenLikeTrace) -> Result<FumenLikeTrace, FumenTransformError> {
        let encoded = FumenLikeWriter::write(trace).map_err(FumenTransformError::Write)?;
        FumenLikeReader::read(&encoded).map_err(FumenTransformError::Read)
    }
}
impl FumenTransformContract {
    pub fn combine(chunks: &[Vec<String>]) -> FumenLikeTrace {
        FumenLikeTrace::new(CombineTransform::combine_pages(chunks))
    }
}
impl FumenTransformContract {
    pub fn split(trace: &FumenLikeTrace) -> Vec<FumenLikeTrace> {
        SplitTransform::split_pages(trace.pages())
            .into_iter()
            .map(FumenLikeTrace::new)
            .collect()
    }
}
impl FumenTransformContract {
    pub fn mirror(trace: &FumenLikeTrace) -> FumenLikeTrace {
        FumenLikeTrace::new(
            trace
                .pages()
                .iter()
                .map(|page| MirrorTransform::transform_page_comment(page))
                .collect(),
        )
    }
}
impl FumenTransformContract {
    pub fn field_mirror(trace: &FumenLikeTrace) -> FumenLikeTrace {
        Self::mirror(trace)
    }
}
impl FumenTransformContract {
    pub fn grayout(trace: &FumenLikeTrace) -> FumenLikeTrace {
        FumenLikeTrace::new(
            trace
                .pages()
                .iter()
                .map(|page| GrayoutTransform::transform_page_comment(page))
                .collect(),
        )
    }
}
impl FumenTransformContract {
    pub fn remove_comments(trace: &FumenLikeTrace) -> FumenLikeTrace {
        FumenLikeTrace::new(
            trace
                .pages()
                .iter()
                .map(|page| {
                    let fields = page
                        .lines()
                        .filter(|line| line.contains('='))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if fields.is_empty() {
                        "kind=comment-removed".to_owned()
                    } else {
                        fields
                    }
                })
                .collect(),
        )
    }
}
impl FumenTransformContract {
    pub fn preserve_comments(trace: &FumenLikeTrace) -> FumenLikeTrace {
        trace.clone()
    }
}
impl FumenTransformContract {
    pub fn page_shift(trace: &FumenLikeTrace, offset: isize) -> FumenLikeTrace {
        FumenLikeTrace::new(PageShiftTransform::shift_pages(trace.pages(), offset))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FumenTransformError {
    Read(FumenLikeReadError),
    Write(FumenLikeWriteError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTemplateDraft {
    template_id: String,
    slot_count: usize,
    mirror_policy: String,
}

impl BuildTemplateDraft {
    pub fn template_id(&self) -> &str {
        &self.template_id
    }
}
impl BuildTemplateDraft {
    pub const fn slot_count(&self) -> usize {
        self.slot_count
    }
}
impl BuildTemplateDraft {
    pub fn mirror_policy(&self) -> &str {
        &self.mirror_policy
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FumenToBuildTemplateAdapter;

impl FumenToBuildTemplateAdapter {
    pub fn build_template_from_fumen(
        input: &str,
    ) -> Result<BuildTemplateDraft, BuildTemplateError> {
        let trace = FumenLikeReader::read(input).map_err(BuildTemplateError::Read)?;
        Self::build_template_from_trace(&trace)
    }
}
impl FumenToBuildTemplateAdapter {
    pub fn build_template_from_trace(
        trace: &FumenLikeTrace,
    ) -> Result<BuildTemplateDraft, BuildTemplateError> {
        let page = trace
            .pages()
            .first()
            .ok_or(BuildTemplateError::MissingPage)?;
        let kind = field(page, "kind").unwrap_or_default();
        if kind != "build-template" {
            return Err(BuildTemplateError::UnsupportedPageKind {
                kind: kind.to_owned(),
            });
        }

        let template_id = field(page, "template_id")
            .filter(|value| !value.is_empty())
            .ok_or(BuildTemplateError::MissingTemplateId)?
            .to_owned();
        let slot_count = field(page, "slot_count")
            .ok_or(BuildTemplateError::MissingSlotCount)?
            .parse()
            .map_err(|_| BuildTemplateError::InvalidSlotCount)?;
        if slot_count == 0 {
            return Err(BuildTemplateError::InvalidSlotCount);
        }

        Ok(BuildTemplateDraft {
            template_id,
            slot_count,
            mirror_policy: field(page, "mirror_policy").unwrap_or("none").to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildTemplateError {
    Read(FumenLikeReadError),
    MissingPage,
    UnsupportedPageKind { kind: String },
    MissingTemplateId,
    MissingSlotCount,
    InvalidSlotCount,
}

fn field<'a>(page: &'a str, key: &str) -> Option<&'a str> {
    page.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate.trim() == key).then(|| value.trim())
    })
}

#[cfg(test)]
#[path = "fumen_transform_contract_tests.rs"]
mod tests;
