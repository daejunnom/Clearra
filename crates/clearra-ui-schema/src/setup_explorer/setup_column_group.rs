use super::setup_result_column_schema::SetupResultColumnSchema;

pub(crate) fn append_column_groups<const N: usize>(
    groups: [Vec<SetupResultColumnSchema>; N],
) -> Vec<SetupResultColumnSchema> {
    groups.into_iter().flatten().collect()
}
