use kaspa_kusd::silverscript::{CompileOptions, compile_contract};
use silverscript_lang::ast::Expr;

#[test]
fn savings_account_compiles_with_current_silverscript() {
    let source = std::fs::read_to_string("contracts/savings-account.sil").unwrap();
    let contract = compile_contract(
        &source,
        &[
            Expr::bytes(vec![0x01; 32]),
            Expr::bytes(vec![0x02; 32]),
            Expr::bytes(vec![0x03; 32]),
            Expr::bytes(vec![0x04; 32]),
            Expr::int(100_000_000_000),
            Expr::int(20_000),
            Expr::int(259_200),
            Expr::int(31_536_000),
            Expr::int(31_536_000),
            Expr::bytes(vec![0x05; 32]),
            Expr::int(0),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![0x06; 32]),
            Expr::int(11),
            Expr::int(11),
            Expr::bytes(vec![0x07; 32]),
            Expr::int(12),
            Expr::int(12),
            Expr::bytes(vec![0x08; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!contract.bytecode.is_empty());
}

#[test]
fn disabled_savings_controller_compiles_with_current_silverscript() {
    let account_source = std::fs::read_to_string("contracts/savings-account.sil").unwrap();
    let account = compile_contract(
        &account_source,
        &[
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::int(100),
            Expr::int(20_000),
            Expr::int(10),
            Expr::int(1000),
            Expr::int(10_000),
            Expr::bytes(vec![5; 32]),
            Expr::int(0),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![6; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![7; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![8; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let layout = account.state_layout;
    let source = std::fs::read_to_string("contracts/savings-controller.sil").unwrap();
    let controller = compile_contract(
        &source,
        &[
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::bytes(vec![9; 32]),
            Expr::bool(false),
            Expr::int(0),
            Expr::int(0),
            Expr::int(0),
            Expr::int(0),
            Expr::int(259_200),
            Expr::int(31_536_000),
            Expr::int(31_536_000),
            Expr::int(100_000_000),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![12; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![13; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![6; 32]),
            Expr::int(layout.start as i64),
            Expr::int((account.bytecode.len() - layout.start - layout.len) as i64),
            Expr::bytes(account.template_hash().to_vec()),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![10; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![11; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!controller.bytecode.is_empty());
}

#[test]
fn governed_savings_proposal_compiles_with_current_silverscript() {
    let source = std::fs::read_to_string("contracts/savings-proposal.sil").unwrap();
    let proposal = compile_contract(
        &source,
        &[
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::int(1),
            Expr::bool(false),
            Expr::int(20_000),
            Expr::int(259_200),
            Expr::int(31_536_000),
            Expr::int(31_536_000),
            Expr::int(604_800),
            Expr::int(604_800),
            Expr::int(100_000_000),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!proposal.bytecode.is_empty());
}

#[test]
fn savings_governor_with_kps_veto_compiles() {
    let proposal_source = std::fs::read_to_string("contracts/savings-proposal.sil").unwrap();
    let proposal = compile_contract(
        &proposal_source,
        &[
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::int(1),
            Expr::bool(false),
            Expr::int(20_000),
            Expr::int(259_200),
            Expr::int(31_536_000),
            Expr::int(31_536_000),
            Expr::int(604_800),
            Expr::int(604_800),
            Expr::int(100_000_000),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let layout = proposal.state_layout;
    let source = std::fs::read_to_string("contracts/savings-governor.sil").unwrap();
    let governor = compile_contract(
        &source,
        &[
            Expr::bytes(vec![4; 32]),
            Expr::bytes(vec![5; 32]),
            Expr::bytes(vec![6; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::int(0),
            Expr::int(0),
            Expr::bytes(vec![0; 32]),
            Expr::int(604_800),
            Expr::int(604_800),
            Expr::int(20_000),
            Expr::int(100_000_000),
            Expr::dynamic_bytes(proposal.bytecode[..layout.start].to_vec()),
            Expr::dynamic_bytes(proposal.bytecode[layout.start + layout.len..].to_vec()),
            Expr::bytes(proposal.template_hash().to_vec()),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![7; 32]),
            Expr::int(11),
            Expr::int(11),
            Expr::bytes(vec![8; 32]),
            Expr::int(12),
            Expr::int(12),
            Expr::bytes(vec![9; 32]),
            Expr::int(4),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!governor.bytecode.is_empty());
}

#[test]
fn reserve_savings_payment_compiles_against_real_account_and_controller_templates() {
    let account_source = std::fs::read_to_string("contracts/savings-account.sil").unwrap();
    let account = compile_contract(
        &account_source,
        &[
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::int(100_000_000_000),
            Expr::int(20_000),
            Expr::int(259_200),
            Expr::int(31_536_000),
            Expr::int(31_536_000),
            Expr::bytes(vec![5; 32]),
            Expr::int(0),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![6; 32]),
            Expr::int(11),
            Expr::int(11),
            Expr::bytes(vec![7; 32]),
            Expr::int(12),
            Expr::int(12),
            Expr::bytes(vec![8; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let al = account.state_layout;
    let controller_source = std::fs::read_to_string("contracts/savings-controller.sil").unwrap();
    let controller = compile_contract(
        &controller_source,
        &[
            Expr::bytes(vec![1; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::bytes(vec![3; 32]),
            Expr::bytes(vec![9; 32]),
            Expr::bool(false),
            Expr::int(0),
            Expr::int(0),
            Expr::int(0),
            Expr::int(0),
            Expr::int(259_200),
            Expr::int(31_536_000),
            Expr::int(31_536_000),
            Expr::int(100_000_000),
            Expr::int(11),
            Expr::int(11),
            Expr::bytes(vec![7; 32]),
            Expr::int(12),
            Expr::int(12),
            Expr::bytes(vec![8; 32]),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![6; 32]),
            Expr::int(al.start as i64),
            Expr::int((account.bytecode.len() - al.start - al.len) as i64),
            Expr::bytes(account.template_hash().to_vec()),
            Expr::int(13),
            Expr::int(13),
            Expr::bytes(vec![10; 32]),
            Expr::int(14),
            Expr::int(14),
            Expr::bytes(vec![11; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let cl = controller.state_layout;
    let reserve_source = std::fs::read_to_string("contracts/equity-reserve.sil").unwrap();
    let reserve = compile_contract(
        &reserve_source,
        &[
            Expr::bytes(vec![4; 32]),
            Expr::bytes(vec![12; 32]),
            Expr::int(100_000_000_000),
            Expr::int(100_000_000_000),
            Expr::int(0),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![6; 32]),
            Expr::int(15),
            Expr::int(15),
            Expr::bytes(vec![13; 32]),
            Expr::int(16),
            Expr::int(16),
            Expr::bytes(vec![14; 32]),
            Expr::bytes(vec![2; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![15; 32]),
            Expr::int(al.start as i64),
            Expr::int((account.bytecode.len() - al.start - al.len) as i64),
            Expr::bytes(account.template_hash().to_vec()),
            Expr::int(cl.start as i64),
            Expr::int((controller.bytecode.len() - cl.start - cl.len) as i64),
            Expr::bytes(controller.template_hash().to_vec()),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert!(!reserve.bytecode.is_empty());

    let (rp, rs, rh) = {
        let l = reserve.state_layout;
        (
            reserve.bytecode[..l.start].to_vec(),
            reserve.bytecode[l.start + l.len..].to_vec(),
            reserve.template_hash().to_vec(),
        )
    };
    let auction_source = std::fs::read_to_string("contracts/auction.sil").unwrap();
    let auction = compile_contract(
        &auction_source,
        &[
            Expr::bytes(vec![15; 32]),
            Expr::bytes(vec![16; 32]),
            Expr::bytes(vec![4; 32]),
            Expr::bytes(vec![18; 32]),
            Expr::int(1_500_000_000),
            Expr::int(150_000_000),
            Expr::bytes(vec![17; 32]),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(100_000_000_000),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![6; 32]),
            Expr::int(rp.len() as i64),
            Expr::int(rs.len() as i64),
            Expr::bytes(rh),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let apl = auction.state_layout;
    let final_reserve = compile_contract(
        &reserve_source,
        &[
            Expr::bytes(vec![4; 32]),
            Expr::bytes(vec![12; 32]),
            Expr::int(100_000_000_000),
            Expr::int(100_000_000_000),
            Expr::int(0),
            Expr::int(10),
            Expr::int(10),
            Expr::bytes(vec![6; 32]),
            Expr::int(15),
            Expr::int(15),
            Expr::bytes(vec![13; 32]),
            Expr::int(apl.start as i64),
            Expr::int((auction.bytecode.len() - apl.start - apl.len) as i64),
            Expr::bytes(auction.template_hash().to_vec()),
            Expr::bytes(vec![2; 32]),
            Expr::int(1),
            Expr::int(1),
            Expr::bytes(vec![15; 32]),
            Expr::int(al.start as i64),
            Expr::int((account.bytecode.len() - al.start - al.len) as i64),
            Expr::bytes(account.template_hash().to_vec()),
            Expr::int(cl.start as i64),
            Expr::int((controller.bytecode.len() - cl.start - cl.len) as i64),
            Expr::bytes(controller.template_hash().to_vec()),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    assert_eq!(reserve.template_hash(), final_reserve.template_hash());
    assert_eq!(reserve.state_layout, final_reserve.state_layout);
}
