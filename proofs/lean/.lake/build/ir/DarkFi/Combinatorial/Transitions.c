// Lean compiler output
// Module: DarkFi.Combinatorial.Transitions
// Imports: Init DarkFi.Combinatorial.StateSpace
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
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTotalTransitionCount(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_trajectoryRatio(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseMutateTransitionCount___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l1TrajectoryCount___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxPutTransitionCount___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseTotalTransitionCount___boxed(lean_object*, lean_object*);
lean_object* lean_nat_div(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l2TrajectoryCount___boxed(lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseBalanceQueryCount(lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseTotalTransitionCount(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseBalanceQueryCount___boxed(lean_object*);
lean_object* lean_nat_pow(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTotalTransitionCount___boxed(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l2TrajectoryCount(lean_object*);
lean_object* lean_nat_mul(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTakeTransitionCount___boxed(lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxPutTransitionCount(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l1TrajectoryCount(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_trajectoryRatio___boxed(lean_object*, lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTakeTransitionCount(lean_object*);
lean_object* lean_nat_add(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseMutateTransitionCount(lean_object*, lean_object*);
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxPutTransitionCount(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = lean_nat_mul(x_1, x_2);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxPutTransitionCount___boxed(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = l_Combinatorial_Transitions_boxPutTransitionCount(x_1, x_2);
lean_dec(x_2);
lean_dec(x_1);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTakeTransitionCount(lean_object* x_1) {
_start:
{
lean_inc(x_1);
return x_1;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTakeTransitionCount___boxed(lean_object* x_1) {
_start:
{
lean_object* x_2; 
x_2 = l_Combinatorial_Transitions_boxTakeTransitionCount(x_1);
lean_dec(x_1);
return x_2;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTotalTransitionCount(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; lean_object* x_4; 
x_3 = lean_nat_mul(x_1, x_2);
x_4 = lean_nat_add(x_3, x_1);
lean_dec(x_3);
return x_4;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_boxTotalTransitionCount___boxed(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = l_Combinatorial_Transitions_boxTotalTransitionCount(x_1, x_2);
lean_dec(x_2);
lean_dec(x_1);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseMutateTransitionCount(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = lean_nat_mul(x_1, x_2);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseMutateTransitionCount___boxed(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = l_Combinatorial_Transitions_purseMutateTransitionCount(x_1, x_2);
lean_dec(x_2);
lean_dec(x_1);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseBalanceQueryCount(lean_object* x_1) {
_start:
{
lean_inc(x_1);
return x_1;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseBalanceQueryCount___boxed(lean_object* x_1) {
_start:
{
lean_object* x_2; 
x_2 = l_Combinatorial_Transitions_purseBalanceQueryCount(x_1);
lean_dec(x_1);
return x_2;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseTotalTransitionCount(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; lean_object* x_4; lean_object* x_5; 
x_3 = lean_nat_mul(x_1, x_2);
x_4 = lean_nat_add(x_3, x_3);
lean_dec(x_3);
x_5 = lean_nat_add(x_4, x_1);
lean_dec(x_4);
return x_5;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_purseTotalTransitionCount___boxed(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = l_Combinatorial_Transitions_purseTotalTransitionCount(x_1, x_2);
lean_dec(x_2);
lean_dec(x_1);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l1TrajectoryCount(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = lean_nat_pow(x_1, x_2);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l1TrajectoryCount___boxed(lean_object* x_1, lean_object* x_2) {
_start:
{
lean_object* x_3; 
x_3 = l_Combinatorial_Transitions_l1TrajectoryCount(x_1, x_2);
lean_dec(x_2);
lean_dec(x_1);
return x_3;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l2TrajectoryCount(lean_object* x_1) {
_start:
{
lean_object* x_2; 
x_2 = lean_unsigned_to_nat(1u);
return x_2;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_l2TrajectoryCount___boxed(lean_object* x_1) {
_start:
{
lean_object* x_2; 
x_2 = l_Combinatorial_Transitions_l2TrajectoryCount(x_1);
lean_dec(x_1);
return x_2;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_trajectoryRatio(lean_object* x_1, lean_object* x_2, lean_object* x_3) {
_start:
{
lean_object* x_4; lean_object* x_5; lean_object* x_6; 
x_4 = lean_nat_pow(x_1, x_2);
x_5 = lean_unsigned_to_nat(1u);
x_6 = lean_nat_div(x_4, x_5);
lean_dec(x_4);
return x_6;
}
}
LEAN_EXPORT lean_object* l_Combinatorial_Transitions_trajectoryRatio___boxed(lean_object* x_1, lean_object* x_2, lean_object* x_3) {
_start:
{
lean_object* x_4; 
x_4 = l_Combinatorial_Transitions_trajectoryRatio(x_1, x_2, x_3);
lean_dec(x_2);
lean_dec(x_1);
return x_4;
}
}
lean_object* initialize_Init(uint8_t builtin, lean_object*);
lean_object* initialize_DarkFi_Combinatorial_StateSpace(uint8_t builtin, lean_object*);
static bool _G_initialized = false;
LEAN_EXPORT lean_object* initialize_DarkFi_Combinatorial_Transitions(uint8_t builtin, lean_object* w) {
lean_object * res;
if (_G_initialized) return lean_io_result_mk_ok(lean_box(0));
_G_initialized = true;
res = initialize_Init(builtin, lean_io_mk_world());
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
res = initialize_DarkFi_Combinatorial_StateSpace(builtin, lean_io_mk_world());
if (lean_io_result_is_error(res)) return res;
lean_dec_ref(res);
return lean_io_result_mk_ok(lean_box(0));
}
#ifdef __cplusplus
}
#endif
