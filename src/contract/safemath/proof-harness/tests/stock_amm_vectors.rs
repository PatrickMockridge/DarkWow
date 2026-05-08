use dwow_engine::{
    zk::{halo2::Value, vm::ZkCircuit, vm_heap::Witness},
    zkas::{Analyzer, Compiler, Lexer, Parser, ZkBinary},
};
use dwow_safemath_zk::{
    host::{cross_mul_gte_u64, floor_div_u128_by_u64_to_u64, floor_sqrt_u128_to_u64, split_u128},
    safemath::stock::{
        helpers::{
            ASSERT_U128_2X64_V1_ZK, ASSERT_U128_LTE_2X64_V1_ZK, ASSERT_U128_LT_2X64_V1_ZK,
            MIN_SELECT_U128_2X64_V1_ZK,
        },
        CROSS_MUL_GTE_U64_V1_ZK, CROSS_MUL_LTE_U64_V1_ZK, DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK,
        SQRT_FLOOR_U128_V1_ZK,
    },
};
use halo2_proofs::{dev::MockProver, pasta::pallas};

fn compile_template(filename: &str, source: &str) -> ZkBinary {
    let tokens = Lexer::new(filename, source.chars()).lex().unwrap();
    let (namespace, k, constants, witnesses, statements) =
        Parser::new(filename, source.chars(), tokens)
            .parse()
            .unwrap();

    let mut analyzer = Analyzer::new(filename, source.chars(), constants, witnesses, statements);
    analyzer.analyze_types().unwrap();

    let bincode = Compiler::new(
        filename,
        source.chars(),
        namespace,
        k,
        analyzer.constants,
        analyzer.witnesses,
        analyzer.statements,
        analyzer.literals,
        false,
    )
    .compile()
    .unwrap();

    ZkBinary::decode(&bincode, false).unwrap()
}

fn witness_base(value: u64) -> Witness {
    Witness::Base(Value::known(pallas::Base::from(value)))
}

fn empty_instances() -> Vec<Vec<pallas::Base>> {
    vec![vec![]]
}

fn assert_template_satisfied(zkbin: &ZkBinary, witnesses: Vec<Witness>) {
    let circuit = ZkCircuit::new(witnesses, zkbin);
    let prover = MockProver::run(zkbin.k, &circuit, empty_instances()).unwrap();
    prover.assert_satisfied();
}

fn prove_div_floor(zkbin: &ZkBinary, numerator: u128, denominator: u64, quotient: u64) {
    let remainder = u64::try_from(numerator % u128::from(denominator)).unwrap();
    let limbs = split_u128(numerator);

    assert_template_satisfied(
        zkbin,
        vec![
            witness_base(limbs.lo),
            witness_base(limbs.hi),
            witness_base(denominator),
            witness_base(quotient),
            witness_base(remainder),
        ],
    );
}

#[test]
fn assert_u128_2x64_vector_round_trips() {
    let zkbin = compile_template("assert_u128_2x64_v1.zk", ASSERT_U128_2X64_V1_ZK);
    let limbs = split_u128((u128::from(u64::MAX) << 64) | 42);

    assert_template_satisfied(&zkbin, vec![witness_base(limbs.lo), witness_base(limbs.hi)]);
}

#[test]
fn initialize_vector_matches_stock_safemath_sqrt_floor() {
    let sqrt_floor = compile_template("sqrt_floor_u128_v1.zk", SQRT_FLOOR_U128_V1_ZK);

    let amount_0_in = 123_456_u64;
    let amount_1_in = 789_012_u64;
    let product = u128::from(amount_0_in) * u128::from(amount_1_in);
    let lp_minted = floor_sqrt_u128_to_u64(product);
    let limbs = split_u128(product);

    assert_eq!(lp_minted, 312_102);

    assert_template_satisfied(
        &sqrt_floor,
        vec![
            witness_base(limbs.lo),
            witness_base(limbs.hi),
            witness_base(lp_minted),
        ],
    );
}

#[test]
fn add_liquidity_vector_matches_stock_safemath_relations() {
    let div_floor = compile_template(
        "div_floor_u128_by_u64_to_u64_v1.zk",
        DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK,
    );
    // `min` is intentionally helper-only in stock v0, but we still exercise it
    // here because the full AMM add-liquidity vector needs the chosen leg bound.
    let min_select = compile_template("min_select_u128_2x64_v1.zk", MIN_SELECT_U128_2X64_V1_ZK);
    let ratio_lte = compile_template("cross_mul_lte_u64_v1.zk", CROSS_MUL_LTE_U64_V1_ZK);

    let reserve_0 = 40_000_000_u64;
    let reserve_1 = 90_000_000_u64;
    let lp_total = 123_456_789_u64;
    let amount_0_in = 1_000_000_u64;
    let amount_1_in = 3_000_000_u64;

    let lp0_num = u128::from(amount_0_in) * u128::from(lp_total);
    let lp1_num = u128::from(amount_1_in) * u128::from(lp_total);
    let lp0 = floor_div_u128_by_u64_to_u64(lp0_num, reserve_0).unwrap();
    let lp1 = floor_div_u128_by_u64_to_u64(lp1_num, reserve_1).unwrap();
    let lp_minted = lp0.min(lp1);

    assert_eq!(lp_minted, 3_086_419);

    prove_div_floor(&div_floor, lp0_num, reserve_0, lp0);
    prove_div_floor(&div_floor, lp1_num, reserve_1, lp1);

    let lp0_limbs = split_u128(u128::from(lp0));
    let lp1_limbs = split_u128(u128::from(lp1));
    let minted_limbs = split_u128(u128::from(lp_minted));

    assert_template_satisfied(
        &min_select,
        vec![
            witness_base(lp0_limbs.lo),
            witness_base(lp0_limbs.hi),
            witness_base(lp1_limbs.lo),
            witness_base(lp1_limbs.hi),
            witness_base(minted_limbs.lo),
            witness_base(minted_limbs.hi),
            witness_base(u64::from(lp0 <= lp1)),
        ],
    );

    assert_template_satisfied(
        &ratio_lte,
        vec![
            witness_base(amount_0_in),
            witness_base(reserve_0),
            witness_base(amount_1_in),
            witness_base(reserve_1),
        ],
    );
}

#[test]
fn remove_liquidity_vector_matches_stock_safemath_div_floor() {
    let div_floor = compile_template(
        "div_floor_u128_by_u64_to_u64_v1.zk",
        DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK,
    );

    let reserve_0 = 40_000_000_u64;
    let reserve_1 = 90_000_000_u64;
    let lp_total = 123_456_789_u64;
    let lp_burn = 2_000_000_u64;

    let amount_0_num = u128::from(lp_burn) * u128::from(reserve_0);
    let amount_1_num = u128::from(lp_burn) * u128::from(reserve_1);
    let amount_0_out = floor_div_u128_by_u64_to_u64(amount_0_num, lp_total).unwrap();
    let amount_1_out = floor_div_u128_by_u64_to_u64(amount_1_num, lp_total).unwrap();

    assert_eq!(amount_0_out, 648_000);
    assert_eq!(amount_1_out, 1_458_000);

    prove_div_floor(&div_floor, amount_0_num, lp_total, amount_0_out);
    prove_div_floor(&div_floor, amount_1_num, lp_total, amount_1_out);
}

#[test]
fn swap_exact_in_vector_matches_stock_safemath_div_floor() {
    let div_floor = compile_template(
        "div_floor_u128_by_u64_to_u64_v1.zk",
        DIV_FLOOR_U128_BY_U64_TO_U64_V1_ZK,
    );

    let reserve_in = 1_000_000_u64;
    let reserve_out = 500_000_u64;
    let amount_in = 10_000_u64;
    let fee_num = 997_u64;
    let fee_den = 1000_u64;

    let amount_eff = u128::from(amount_in) * u128::from(fee_num);
    let numerator = amount_eff * u128::from(reserve_out);
    let denominator = u128::from(reserve_in) * u128::from(fee_den) + amount_eff;
    let amount_out =
        floor_div_u128_by_u64_to_u64(numerator, u64::try_from(denominator).unwrap()).unwrap();

    assert_eq!(amount_out, 4_935);

    prove_div_floor(
        &div_floor,
        numerator,
        u64::try_from(denominator).unwrap(),
        amount_out,
    );
}

#[test]
fn stock_compare_helpers_match_expected_ordering() {
    let lt = compile_template("assert_u128_lt_2x64_v1.zk", ASSERT_U128_LT_2X64_V1_ZK);
    let lte = compile_template("assert_u128_lte_2x64_v1.zk", ASSERT_U128_LTE_2X64_V1_ZK);

    let smaller = split_u128((u128::from(3_u64) << 64) | 7);
    let larger = split_u128((u128::from(5_u64) << 64) | 1);

    assert_template_satisfied(
        &lt,
        vec![
            witness_base(smaller.lo),
            witness_base(smaller.hi),
            witness_base(larger.lo),
            witness_base(larger.hi),
        ],
    );

    assert_template_satisfied(
        &lte,
        vec![
            witness_base(smaller.lo),
            witness_base(smaller.hi),
            witness_base(smaller.lo),
            witness_base(smaller.hi),
        ],
    );
}

#[test]
fn stock_cross_mul_gte_relation_matches_expected_vectors() {
    let gte = compile_template("cross_mul_gte_u64_v1.zk", CROSS_MUL_GTE_U64_V1_ZK);

    assert!(cross_mul_gte_u64(4_935, 10_000, 0, 1).unwrap());
    assert!(cross_mul_gte_u64(1_000, 1, 4_000, 5).unwrap());

    assert_template_satisfied(
        &gte,
        vec![
            witness_base(4_935),
            witness_base(10_000),
            witness_base(0),
            witness_base(1),
        ],
    );

    assert_template_satisfied(
        &gte,
        vec![
            witness_base(1_000),
            witness_base(1),
            witness_base(4_000),
            witness_base(5),
        ],
    );
}
