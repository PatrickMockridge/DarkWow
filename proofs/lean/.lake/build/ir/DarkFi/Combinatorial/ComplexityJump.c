// Lean compiler output
// Module: DarkFi.Combinatorial.ComplexityJump
// Imports: Init DarkFi.Combinatorial.StateSpace DarkFi.Combinatorial.Transitions
#include <lean/lean.h>
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wunused-parameter"
#pragma clang diagnostic ignored "-Wunused-label"
#elif defined(__GNUC__) && !defined(__CLANG__)
#pragma GCC diagnostic ignored "-Wunused-parameter"
#pragma GCC diagnostic ignored "-Wunused-label"
#pragma GCC diagnostic ignored "-Wunused-but-set-variable"
#endif
#ifdef __cplusplus
extern "C" {
#endif
lean_object* l_Combinatorial_Transitions_boxTotalTransitionCount(lean_object*, lean_object*);
static lean_object* l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__1;
static lean_object* l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__2;
LEAN_EXPORT uint8_t l_Combinatorial_ComplexityJump_l1__information__theoretic__privacy___nativeDecide__1;
LEAN_EXPORT lean_object* l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions;
static lean_object* _init_l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__1() {
_start:
{
lean_object* x_1; 
x_1 = lean_cstr_to_nat("18446744073709551616");
return x_1;
}
}
static lean_object* _init_l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__2() {
_start:
{
lean_object* x_1; lean_object* x_2; lean_object* x_3; 
x_1 = lean_cstr_to_nat("4294967296");
x_2 = l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__1;
x_3 = l_Combinatorial_Transitions_boxTotalTransitionCount(x_1, x_2);
return x_3;
}
}
static lean_object* _init_l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions() {
_start:
{
lean_object* x_1; 
x_1 = l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__2;
return x_1;
}
}
static uint8_t _init_l_Combinatorial_ComplexityJump_l1__information__theoretic__privacy___nativeDecide__1() {
_start:
{
uint8_t x_1; 
x_1 = 1;
return x_1;
}
}
lean_object* initialize_Init(uint8_t builtin, lean_object*);
lean_object* initialize_DarkFi_Combinatorial_StateSpace(uint8_t builtin, lean_object*);
lean_object* initialize_DarkFi_Combinatorial_Transitions(uint8_t builtin, lean_object*);
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_DarkFi_Combinatorial_ComplexityJump(uint8_t builtin, lean_object* w) {
lean_object * res;
if (_G_initialized) return lean_io_result_mk_ok(lean_box(0));
_G_initialized = true;
res = initialize_Init(builtin, lean_io_mk_world());
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_DarkFi_Combinatorial_StateSpace(builtin, lean_io_mk_world());
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_DarkFi_Combinatorial_Transitions(builtin, lean_io_mk_world());
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__1 = _init_l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__1();
lean_mark_persistent(l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__1);
l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__2 = _init_l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__2();
lean_mark_persistent(l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions___closed__2);
l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions = _init_l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions();
lean_mark_persistent(l_Combinatorial_ComplexityJump_theoreticalMaxBoxTransitions);
l_Combinatorial_ComplexityJump_l1__information__theoretic__privacy___nativeDecide__1 = _init_l_Combinatorial_ComplexityJump_l1__information__theoretic__privacy___nativeDecide__1();
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
