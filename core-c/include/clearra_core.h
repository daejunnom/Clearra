#ifndef CLEARRA_CORE_H
#define CLEARRA_CORE_H

#include "clr_memory.h"
#include "clr_gpu.h"
#include "clr_execution_control.h"
#include "clr_problem.h"
#include "clr_resource_budget.h"

#ifdef __cplusplus
extern "C" {
#endif

#define CLEARRA_CORE_ABI_VERSION 21

const char *clearra_core_version(void);
int clearra_core_abi_version(void);

#ifdef __cplusplus
}
#endif

#endif
