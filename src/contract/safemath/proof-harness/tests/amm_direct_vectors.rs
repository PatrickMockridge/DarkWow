use darkfi_engine::{
    zk::{halo2::Value, vm::ZkCircuit, vm_heap::Witness},
    zkas::{Analyzer, Compiler, Lexer, Parser, ZkBinary},
};
use darkfi_safemath_zk::safemath::experimental::{
    DIV_FLOOR_V1_ZK, MIN_SELECT_V1_ZK, RATIO_LTE_V1_ZK, SQRT_FLOOR_V1_ZK,
};
use halo2_proofs::{dev::MockProver, pasta::pallas};

// These are real proof-validation tests, but stock official DarkFi at the pinned
// revision still rejects the widened range profiles used by this safemath set.
// Keep them opt-in so the default portable harness path stays green.

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

fn floor_div_u128(n: u128, d: u128) -> u128 {
    assert!(d > 0);
    n / d
}

fn floor_sqrt_u128(n: u128) -> u128 {
    if n < 2 {
        return n;
    }

    let mut lo = 1_u128;
    let mut hi = 1_u128 << 64;

    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    lo
}

fn prove_div_floor(zkbin: &ZkBinary, numerator: u64, denominator: u64, quotient: u64) {
    let remainder = numerator % denominator;
    assert_eq!(
        u128::from(numerator),
        u128::from(denominator) * u128::from(quotient) + u128::from(remainder)
    );

    assert_template_satisfied(
        zkbin,
        vec![
            witness_base(numerator),
            witness_base(denominator),
            witness_base(quotient),
            witness_base(remainder),
        ],
    );
}

#[test]
#[ignore = "requires DarkFi support for widened range_check(126|128|252) profiles"]
fn initialize_vector_matches_safemath_sqrt_floor() {
    let sqrt_floor = compile_template("sqrt_floor_v1.zk", SQRT_FLOOR_V1_ZK);

    let amount_0_in = 123_456_u64;
    let amount_1_in = 789_012_u64;
    let product = u128::from(amount_0_in) * u128::from(amount_1_in);
    let lp_minted = u64::try_from(floor_sqrt_u128(product)).unwrap();

    assert_eq!(lp_minted, 312_102);

    assert_template_satisfied(
        &sqrt_floor,
        vec![
            witness_base(u64::try_from(product).unwrap()),
            witness_base(lp_minted),
        ],
    );
}

#[test]
#[ignore = "requires DarkFi support for widened range_check(126|128|252) profiles"]
fn add_liquidity_vector_matches_safemath_relations() {
    let div_floor = compile_template("div_floor_v1.zk", DIV_FLOOR_V1_ZK);
    let min_select = compile_template("min_select_v1.zk", MIN_SELECT_V1_ZK);
    let ratio_lte = compile_template("ratio_lte_v1.zk", RATIO_LTE_V1_ZK);

    let reserve_0 = 40_000_000_u64;
    let reserve_1 = 90_000_000_u64;
    let lp_total = 123_456_789_u64;
    let amount_0_in = 1_000_000_u64;
    let amount_1_in = 3_000_000_u64;

    let lp0_num = amount_0_in as u128 * lp_total as u128;
    let lp1_num = amount_1_in as u128 * lp_total as u128;
    let lp0 = u64::try_from(floor_div_u128(lp0_num, reserve_0 as u128)).unwrap();
    let lp1 = u64::try_from(floor_div_u128(lp1_num, reserve_1 as u128)).unwrap();
    let lp_minted = lp0.min(lp1);

    assert_eq!(lp_minted, 3_086_419);

    prove_div_floor(&div_floor, u64::try_from(lp0_num).unwrap(), reserve_0, lp0);
    prove_div_floor(&div_floor, u64::try_from(lp1_num).unwrap(), reserve_1, lp1);

    assert_template_satisfied(
        &min_select,
        vec![
            witness_base(lp0),
            witness_base(lp1),
            witness_base(lp_minted),
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
#[ignore = "requires DarkFi support for widened range_check(126|128|252) profiles"]
fn remove_liquidity_vector_matches_safemath_div_floor() {
    let div_floor = compile_template("div_floor_v1.zk", DIV_FLOOR_V1_ZK);

    let reserve_0 = 40_000_000_u64;
    let reserve_1 = 90_000_000_u64;
    let lp_total = 123_456_789_u64;
    let lp_burn = 2_000_000_u64;

    let amount_0_num = lp_burn as u128 * reserve_0 as u128;
    let amount_1_num = lp_burn as u128 * reserve_1 as u128;
    let amount_0_out = u64::try_from(floor_div_u128(amount_0_num, lp_total as u128)).unwrap();
    let amount_1_out = u64::try_from(floor_div_u128(amount_1_num, lp_total as u128)).unwrap();

    assert_eq!(amount_0_out, 648_000);
    assert_eq!(amount_1_out, 1_458_000);

    prove_div_floor(
        &div_floor,
        u64::try_from(amount_0_num).unwrap(),
        lp_total,
        amount_0_out,
    );
    prove_div_floor(
        &div_floor,
        u64::try_from(amount_1_num).unwrap(),
        lp_total,
        amount_1_out,
    );
}

#[test]
#[ignore = "requires DarkFi support for widened range_check(126|128|252) profiles"]
fn swap_exact_in_vector_matches_safemath_div_floor() {
    let div_floor = compile_template("div_floor_v1.zk", DIV_FLOOR_V1_ZK);

    let reserve_in = 1_000_000_u64;
    let reserve_out = 500_000_u64;
    let amount_in = 10_000_u64;
    let fee_num = 997_u64;
    let fee_den = 1000_u64;

    let amount_eff = amount_in as u128 * fee_num as u128;
    let numerator = amount_eff * reserve_out as u128;
    let denominator = reserve_in as u128 * fee_den as u128 + amount_eff;
    let amount_out = u64::try_from(floor_div_u128(numerator, denominator)).unwrap();

    assert_eq!(amount_out, 4_935);

    prove_div_floor(
        &div_floor,
        u64::try_from(numerator).unwrap(),
        u64::try_from(denominator).unwrap(),
        amount_out,
    );
}
