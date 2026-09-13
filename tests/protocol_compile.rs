use kaspa_kusd::silverscript::{CompileOptions, compile_contract};
use silverscript_lang::ast::Expr;

#[test]
fn fee_aware_module_and_kps_compile_with_current_silverscript() {
    let auction_source = std::fs::read_to_string("contracts/auction.sil").unwrap();
    let auction = compile_contract(
        &auction_source,
        &[
            Expr::bytes(vec![0x09; 32]),
            Expr::bytes(vec![0x01; 32]),
            Expr::bytes(vec![0x02; 32]),
            Expr::bytes(vec![0x11; 32]),
            Expr::int(1_500_000_000),
            Expr::int(150_000_000),
            Expr::bytes(vec![0x22; 32]),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(100_000_000_000),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x33; 32]),
            Expr::int(12),
            Expr::int(12),
            Expr::bytes(vec![0x44; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!auction.bytecode.is_empty());
    let al = auction.state_layout;
    let ap = auction.bytecode[..al.start].to_vec();
    let as_ = auction.bytecode[al.start + al.len..].to_vec();
    let ah = auction.template_hash().to_vec();
    let challenge_source = std::fs::read_to_string("contracts/challenge.sil").unwrap();
    let challenge = compile_contract(
        &challenge_source,
        &[
            Expr::bytes(vec![9; 32]),
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::bytes(vec![0x11; 32]),
            Expr::int(1_500_000_000),
            Expr::int(150_000_000),
            Expr::bytes(vec![0x22; 32]),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(3_600),
            Expr::int(100_000_000_000),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x33; 32]),
            Expr::dynamic_bytes(ap),
            Expr::dynamic_bytes(as_),
            Expr::bytes(ah),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let cl = challenge.state_layout;
    let anchor_source = std::fs::read_to_string("contracts/challenge-anchor.sil").unwrap();
    let anchor = compile_contract(
        &anchor_source,
        &[
            Expr::bytes(vec![9; 32]),
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::bytes(vec![0x11; 32]),
            Expr::int(1_500_000_000),
            Expr::int(150_000_000),
            Expr::bytes(vec![0x22; 32]),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(3_600),
            Expr::int(100_000_000_000),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x33; 32]),
            Expr::dynamic_bytes(challenge.bytecode[..cl.start].to_vec()),
            Expr::dynamic_bytes(challenge.bytecode[cl.start + cl.len..].to_vec()),
            Expr::bytes(challenge.template_hash().to_vec()),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!challenge.bytecode.is_empty() && !anchor.bytecode.is_empty());

    let reserve_source = std::fs::read_to_string("contracts/equity-reserve-base.sil").unwrap();
    let reserve = compile_contract(
        &reserve_source,
        &[
            Expr::bytes(vec![0x11; 32]),
            Expr::bytes(vec![0x12; 32]),
            Expr::int(1_515_000_000),
            Expr::int(1_515_000_000),
            Expr::int(100_000_000_000),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x21; 32]),
            Expr::int(11),
            Expr::int(11),
            Expr::bytes(vec![0x22; 32]),
            Expr::int(12),
            Expr::int(12),
            Expr::bytes(vec![0x23; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    eprintln!(
        "EquityReserveBase bytecode: {} bytes",
        reserve.bytecode.len()
    );
    assert!(!reserve.bytecode.is_empty());

    let position_source = std::fs::read_to_string("contracts/position.sil").unwrap();
    let position = compile_contract(
        &position_source,
        &[
            Expr::bytes(vec![0x02; 32]),
            Expr::bytes(vec![0x11; 32]),
            Expr::int(1_500_000_000),
            Expr::int(150_000_000),
            Expr::bytes(vec![0x22; 32]),
            Expr::int(100_000),
            Expr::int(3_500_000),
            Expr::int(3_600),
            Expr::int(3_600),
            Expr::int(10_000),
            Expr::bytes(vec![0; 32]),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x33; 32]),
            Expr::int(0),
            Expr::int(0),
            Expr::bytes(vec![0; 32]),
            Expr::dynamic_bytes(vec![]),
            Expr::dynamic_bytes(vec![]),
            Expr::bytes(vec![0; 32]),
            Expr::dynamic_bytes(vec![]),
            Expr::dynamic_bytes(vec![]),
            Expr::bytes(vec![0; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!position.bytecode.is_empty());

    let module_source = std::fs::read_to_string("contracts/minting-module.sil").unwrap();
    let module = compile_contract(
        &module_source,
        &[
            Expr::bytes(vec![0x11; 32]),
            Expr::int(100_000_000_000_000),
            Expr::int(1_000_000_000_000),
            Expr::int(100_000_000),
            Expr::int(3_500_000),
            Expr::int(499_000_000_000),
            Expr::int(3_600),
            Expr::int(3_600),
            Expr::int(10_000),
            Expr::int(0),
            Expr::bytes(vec![0x22; 32]),
            Expr::int(100_000),
            Expr::int(20_000),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x33; 32]),
            Expr::bytes(vec![0x44; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!module.bytecode.is_empty());

    let kps_source = std::fs::read_to_string("contracts/kps.sil").unwrap();
    let kps = compile_contract(
        &kps_source,
        &[
            Expr::bytes(vec![0x55; 32]),
            Expr::int(0),
            Expr::byte(2),
            Expr::bool(true),
            Expr::int(8),
            Expr::int(8),
            Expr::int(7_776_000),
            Expr::dynamic_bytes(vec![]),
            Expr::dynamic_bytes(vec![]),
            Expr::bytes(vec![0; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!kps.bytecode.is_empty());
}
