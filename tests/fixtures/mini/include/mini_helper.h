#pragma once
/* Helper header: no compiled object, so the scheduler must skip it
   (out_of_scope / outside_scope) instead of halting in substitute. */
#define MINI_HELPER_MAGIC 42
