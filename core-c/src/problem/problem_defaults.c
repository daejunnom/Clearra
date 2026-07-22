#include "clr_problem.h"

clr_problem_budget clr_problem_budget_zero(void) {
    clr_problem_budget budget = {0};
    return budget;
}

clr_checkpoint_spec clr_checkpoint_spec_none(void) {
    clr_checkpoint_spec checkpoint = {0};
    return checkpoint;
}
