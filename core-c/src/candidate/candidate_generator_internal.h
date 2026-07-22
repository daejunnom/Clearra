#ifndef CLEARRA_CANDIDATE_GENERATOR_INTERNAL_H
#define CLEARRA_CANDIDATE_GENERATOR_INTERNAL_H

#include "candidate.h"
ClearraCandidateStatus clearra_candidate_status_from_board_status(
    ClearraBoard64Status status);
ClearraCandidateStatus clearra_candidate_status_from_operation_status(
    ClearraOperationStatus status);
#endif
