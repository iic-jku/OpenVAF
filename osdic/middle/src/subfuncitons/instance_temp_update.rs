/*
 *  ******************************************************************************************
 *  Copyright (c) 2021 Pascal Kuthe. This file is part of the frontend project.
 *  It is subject to the license terms in the LICENSE file found in the top-level directory
 *  of this distribution and at  https://gitlab.com/DSPOM/OpenVAF/blob/master/LICENSE.
 *  No part of frontend, including this file, may be copied, modified, propagated, or
 *  distributed except according to the terms contained in the LICENSE file.
 *  *****************************************************************************************
 */

use crate::frontend::{GeneralOsdiCall, GeneralOsdiInput};
use crate::storage_locations::{StorageLocation, StorageLocations};
use crate::subfuncitons::automatic_slicing::function_cfg_from_full_cfg;
use openvaf_data_structures::index_vec::{IndexSlice, IndexVec};
use openvaf_data_structures::{bit_set::BitSet, HashMap};
use openvaf_hir::Unknown;
use openvaf_ir::ids::{PortId, VariableId};
use openvaf_ir::Type;
use openvaf_middle::cfg::{ControlFlowGraph, IntLocation, InternedLocations};
use openvaf_middle::derivatives::RValueAutoDiff;
use openvaf_middle::dfa::lattice::FlatSet;
use openvaf_middle::{
    COperand, COperandData, CallArg, CfgConversion, CfgFunctions, CfgInputs, Derivative, Mir,
    OperandData, ParameterInput, RValue, StmntKind, VariableLocalKind,
};
use openvaf_pass::program_dependence::{InvProgramDependenceGraph, ProgramDependenceGraph};
use openvaf_session::sourcemap::Span;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use tracing::debug_span;

#[derive(PartialEq, Eq, Clone)]
pub enum InstanceTempUpdateCallType {}

impl CfgFunctions for InstanceTempUpdateCallType {
    type I = InstanceTempUpdateInput;

    fn const_fold(&self, _call: &[FlatSet]) -> FlatSet {
        match *self {}
    }
    fn derivative<C: CfgFunctions>(
        &self,
        _args: &IndexSlice<CallArg, [COperand<Self>]>,
        _ad: &mut RValueAutoDiff<Self, C>,
        _span: Span,
    ) -> Option<RValue<Self>> {
        match *self {}
    }
}

impl Display for InstanceTempUpdateCallType {
    fn fmt(&self, _f: &mut Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl Debug for InstanceTempUpdateCallType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InstanceTempUpdateInput {
    Parameter(ParameterInput),
    PortConnected(PortId),
    Temperature,
}

impl Display for InstanceTempUpdateInput {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameter(param_input) => Display::fmt(param_input, f),
            Self::PortConnected(port) => write!(f, "$port_connected({:?})", port),
            Self::Temperature => write!(f, "$temperature"),
        }
    }
}

impl CfgInputs for InstanceTempUpdateInput {
    fn derivative<C: CfgFunctions>(&self, _unknown: Unknown, _mir: &Mir<C>) -> Derivative<Self> {
        unreachable!() // No derivatives allows in the init function since that would require values that depend uponm voltages
    }

    fn ty<C: CfgFunctions>(&self, mir: &Mir<C>) -> Type {
        match self {
            Self::Parameter(ParameterInput::Value(param)) => mir[*param].ty,
            Self::Parameter(ParameterInput::Given(_)) | Self::PortConnected(_) => Type::BOOL,
            Self::Temperature => Type::REAL,
        }
    }
}

pub struct GeneralToInstanceTempUpdate;

impl CfgConversion<GeneralOsdiCall, InstanceTempUpdateCallType> for GeneralToInstanceTempUpdate {
    fn map_input(
        &mut self,
        src: <GeneralOsdiCall as CfgFunctions>::I,
    ) -> COperandData<InstanceTempUpdateCallType> {
        let input = match src {
            GeneralOsdiInput::Parameter(x) => InstanceTempUpdateInput::Parameter(x),
            GeneralOsdiInput::PortConnected(port) => InstanceTempUpdateInput::PortConnected(port),
            GeneralOsdiInput::Temperature => InstanceTempUpdateInput::Temperature,
            _ => unreachable!(),
        };

        OperandData::Read(input)
    }

    fn map_call_val(
        &mut self,
        _call: GeneralOsdiCall,
        _args: IndexVec<CallArg, COperand<GeneralOsdiCall>>,
        _span: Span,
    ) -> RValue<InstanceTempUpdateCallType> {
        unreachable!()
    }

    fn map_call_stmnt(
        &mut self,
        _call: GeneralOsdiCall,
        _args: IndexVec<CallArg, COperand<GeneralOsdiCall>>,
        _span: Span,
    ) -> StmntKind<InstanceTempUpdateCallType> {
        unreachable!()
    }
}

pub struct InstanceTempUpdateFunction {
    pub cfg: ControlFlowGraph<InstanceTempUpdateCallType>,
    pub written_storage: BitSet<StorageLocation>,
    pub read_storage: BitSet<StorageLocation>,
}

impl InstanceTempUpdateFunction {
    pub fn new(
        cfg: &ControlFlowGraph<GeneralOsdiCall>,
        tainted_locations: &BitSet<IntLocation>,
        assumed_locations: &BitSet<IntLocation>,
        locations: &InternedLocations,
        pdg: &ProgramDependenceGraph,
        inv_pdg: &InvProgramDependenceGraph,
        all_output_stmnts: &BitSet<IntLocation>,
        storage: &StorageLocations,
    ) -> (Self, BitSet<IntLocation>) {
        let _span = debug_span!("Instance Temp Update Function Creation");
        let _enter = _span.enter();

        let (cfg, function_output_locations, written_vars, read_vars) = function_cfg_from_full_cfg(
            cfg,
            tainted_locations,
            Some(assumed_locations),
            all_output_stmnts,
            locations,
            inv_pdg,
            pdg,
            storage,
        );

        let cfg = cfg.map(&mut GeneralToInstanceTempUpdate);

        (
            Self {
                cfg,
                written_storage: written_vars,
                read_storage: read_vars,
            },
            function_output_locations,
        )
    }
}
