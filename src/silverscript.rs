//! Compatibility boundary around SilverScript's portable 1.0 ABI.
//!
//! The protocol keeps using SilverScript AST expressions internally while all
//! signature scripts are encoded through the validated portable ABI.

use silverscript_abi::{
    ArtifactValue, SilAbiArtifact, TypeArtifact, encode_contract_covenant_decl_sig_script,
    encode_contract_entry_sig_script,
};
use silverscript_lang::ast::{Expr, ExprKind};
use silverscript_lang::compiler::{self, CompilerError, sil_abi_artifact_from_compiled};
use std::collections::BTreeMap;
use std::ops::Deref;

pub use silverscript_lang::compiler::{CompileOptions, CovenantDeclCallOptions, struct_object};

/// A compiled contract coupled to the portable ABI that validates its calls.
#[derive(Debug)]
pub struct CompiledContract<'i> {
    compiled: compiler::CompiledContract<'i>,
    abi: SilAbiArtifact,
}

impl<'i> Deref for CompiledContract<'i> {
    type Target = compiler::CompiledContract<'i>;

    fn deref(&self) -> &Self::Target {
        &self.compiled
    }
}

pub fn compile_contract<'i>(
    source: &'i str,
    constructor_args: &[Expr<'i>],
    options: CompileOptions,
) -> Result<CompiledContract<'i>, CompilerError> {
    let compiled = compiler::compile_contract(source, constructor_args, options)?;
    let abi = sil_abi_artifact_from_compiled(&compiled, constructor_args)?;
    abi.check_consistency().map_err(|err| {
        CompilerError::Unsupported(format!("portable ABI verification failed: {err}"))
    })?;
    Ok(CompiledContract { compiled, abi })
}

impl CompiledContract<'_> {
    pub fn build_sig_script(
        &self,
        entry_name: &str,
        args: Vec<Expr<'_>>,
    ) -> Result<Vec<u8>, CompilerError> {
        let contract = self
            .abi
            .contract(&self.compiled.contract_name)
            .ok_or_else(|| {
                CompilerError::Unsupported("compiled contract is absent from its ABI".into())
            })?;
        let entry = contract.entry(entry_name).ok_or_else(|| {
            CompilerError::Unsupported(format!("entry '{entry_name}' is absent from the ABI"))
        })?;
        let values = expr_values(args, entry.params.iter().map(|param| &param.ty), &self.abi)?;
        encode_contract_entry_sig_script(
            &self.abi,
            &self.compiled.contract_name,
            entry_name,
            &values,
        )
        .map_err(codec_error)
    }

    pub fn build_sig_script_for_covenant_decl(
        &self,
        declaration_name: &str,
        args: Vec<Expr<'_>>,
        options: CovenantDeclCallOptions,
    ) -> Result<Vec<u8>, CompilerError> {
        let contract = self
            .abi
            .contract(&self.compiled.contract_name)
            .ok_or_else(|| {
                CompilerError::Unsupported("compiled contract is absent from its ABI".into())
            })?;
        let entry = contract
            .covenant_decl_entry(declaration_name, options.is_leader)
            .ok_or_else(|| {
                CompilerError::Unsupported(format!(
                    "covenant declaration '{declaration_name}' is absent from the ABI"
                ))
            })?;
        let values = expr_values(args, entry.params.iter().map(|param| &param.ty), &self.abi)?;
        encode_contract_covenant_decl_sig_script(
            &self.abi,
            &self.compiled.contract_name,
            declaration_name,
            options.is_leader,
            &values,
        )
        .map_err(codec_error)
    }
}

fn codec_error(err: silverscript_abi::CodecError) -> CompilerError {
    CompilerError::Unsupported(format!("portable ABI call encoding failed: {err}"))
}

fn expr_values<'a>(
    args: Vec<Expr<'_>>,
    types: impl ExactSizeIterator<Item = &'a TypeArtifact>,
    abi: &SilAbiArtifact,
) -> Result<Vec<ArtifactValue>, CompilerError> {
    if args.len() != types.len() {
        return Err(CompilerError::Unsupported(format!(
            "ABI argument count mismatch: expected {}, got {}",
            types.len(),
            args.len()
        )));
    }
    args.into_iter()
        .zip(types)
        .map(|(expr, ty)| expr_value(expr, ty, abi))
        .collect()
}

fn expr_value(
    expr: Expr<'_>,
    ty: &TypeArtifact,
    abi: &SilAbiArtifact,
) -> Result<ArtifactValue, CompilerError> {
    match (expr.kind, ty) {
        (
            ExprKind::Int(value) | ExprKind::Temporal(value) | ExprKind::DateLiteral(value),
            TypeArtifact::Int | TypeArtifact::Temporal,
        ) => Ok(ArtifactValue::Int(value)),
        (ExprKind::Bool(value), TypeArtifact::Bool) => Ok(ArtifactValue::Bool(value)),
        (ExprKind::Byte(value), TypeArtifact::Byte) => Ok(ArtifactValue::Byte(value)),
        (ExprKind::String(value), TypeArtifact::Text) => Ok(ArtifactValue::Text(value)),
        (
            ExprKind::Array { values, .. },
            TypeArtifact::Bytes
            | TypeArtifact::FixedBytes { .. }
            | TypeArtifact::Pubkey
            | TypeArtifact::Sig
            | TypeArtifact::Datasig,
        ) => values
            .into_iter()
            .map(|value| match value.kind {
                ExprKind::Byte(byte) => Ok(byte),
                other => Err(CompilerError::Unsupported(format!(
                    "ABI byte string contains non-byte expression {other:?}"
                ))),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(ArtifactValue::Bytes),
        (ExprKind::Array { values, .. }, TypeArtifact::FixedArray { item, .. })
        | (ExprKind::Array { values, .. }, TypeArtifact::DynamicArray { item }) => values
            .into_iter()
            .map(|value| expr_value(value, item, abi))
            .collect::<Result<Vec<_>, _>>()
            .map(ArtifactValue::Array),
        (ExprKind::StructLiteral { fields, .. }, TypeArtifact::Struct { name }) => fields
            .into_iter()
            .map(|field| {
                let declared_type = struct_field_type(abi, name, &field.name).ok_or_else(|| {
                    CompilerError::Unsupported(format!(
                        "field '{}.{}' is absent from the ABI",
                        name, field.name
                    ))
                })?;
                Ok((field.name, expr_value(field.expr, declared_type, abi)?))
            })
            .collect::<Result<BTreeMap<_, _>, CompilerError>>()
            .map(ArtifactValue::Object),
        (other, expected) => Err(CompilerError::Unsupported(format!(
            "signature and constructor argument {other:?} does not match ABI type {expected:?}"
        ))),
    }
}

fn struct_field_type<'a>(
    abi: &'a SilAbiArtifact,
    struct_name: &str,
    field_name: &str,
) -> Option<&'a TypeArtifact> {
    if let Some(structure) = abi.structs.get(struct_name) {
        return structure
            .fields
            .iter()
            .find(|field| field.name == field_name)
            .map(|field| &field.ty);
    }
    abi.contracts
        .values()
        .find(|contract| contract.runtime_state.source == struct_name)?
        .runtime_state
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .map(|field| &field.ty)
}
