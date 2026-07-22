use crate::{
    bag::{bag_profile::BagProfile, standard_7bag::standard_7_bag_profile},
    board::{board_profile::BoardProfile, standard10::standard_10_board_profile},
    pieces::{
        piece_set_profile::PieceSetProfile,
        standard_tetrominoes::standard_tetromino_piece_set_profile,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StandardProfileBundle {
    board: BoardProfile,
    piece_set: PieceSetProfile,
    bag: BagProfile,
}

impl StandardProfileBundle {
    pub fn standard() -> Self {
        Self {
            board: standard_10_board_profile(),
            piece_set: standard_tetromino_piece_set_profile(),
            bag: standard_7_bag_profile(),
        }
    }
}
impl StandardProfileBundle {
    pub fn board(self) -> BoardProfile {
        self.board
    }
}
impl StandardProfileBundle {
    pub fn piece_set(self) -> PieceSetProfile {
        self.piece_set
    }
}
impl StandardProfileBundle {
    pub fn bag(self) -> BagProfile {
        self.bag
    }
}

pub fn standard_profile_bundle() -> StandardProfileBundle {
    StandardProfileBundle::standard()
}
