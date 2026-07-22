use clearra_build_coverage::template::BuildTemplate;

use super::build_cell_schema::BuildCellSchema;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildPreviewBoardSchema {
    width: u16,
    height: u16,
    occupied_cells: Vec<BuildCellSchema>,
}

impl BuildPreviewBoardSchema {
    pub fn from_template(template: &BuildTemplate) -> Self {
        Self {
            width: template.board_size().width(),
            height: template.board_size().height(),
            occupied_cells: template
                .slots()
                .iter()
                .flat_map(|slot| slot.cells().iter().copied())
                .map(BuildCellSchema::from_coord)
                .collect(),
        }
    }
}
impl BuildPreviewBoardSchema {
    pub fn width(&self) -> u16 {
        self.width
    }
}
impl BuildPreviewBoardSchema {
    pub fn height(&self) -> u16 {
        self.height
    }
}
impl BuildPreviewBoardSchema {
    pub fn occupied_cells(&self) -> &[BuildCellSchema] {
        &self.occupied_cells
    }
}
