use std::collections::HashMap;

use super::factoid::Factoid;
use super::{InferenceFact, InferenceModel, InferenceNode, InferenceOp};
use crate::errors::*;
use crate::hir::ops::dummy::Dummy;
use crate::model::dsl::ModelSpecialOps;
use crate::model::translator::Translate;
use crate::model::NormalizedModel;
use crate::model::{compact, OutletId};
use crate::model::{TypedFact, TypedModel, TypedOp};
use crate::ops::Op;
use crate::prelude::TVec;

impl InferenceModel {
    /// Analyse all nodes of the graph.
    ///
    /// Will stop on first error unless `obstinate` is `true`.
    pub fn analyse(&mut self, obstinate: bool) -> TractResult<bool> {
        super::analyser::Analyser::new(self).analyse_obstinate(obstinate)
    }

    /// Perform early transformation before going typed.
    pub fn incorporate(self) -> TractResult<InferenceModel> {
        let mut model = self;
        loop {
            let mut done_something = false;
            for p in crate::infer::optim::incorporate() {
                done_something = done_something || p.pass(&mut model)?;
                if cfg!(debug_assertions) {
                    model.check_edges()?;
                }
            }
            if !done_something {
                break;
            }
        }
        model = compact::compact(&model)?;
        model.analyse(false)?;
        Ok(model)
    }

    /// List OutletId with incomplete type information.
    ///
    /// Will stop on first error unless `obstinate` is `true`.
    pub fn missing_type_shape(&self) -> TractResult<Vec<OutletId>> {
        Ok(self
            .eval_order()?
            .iter()
            .flat_map(|&node| {
                self.nodes()[node]
                    .outputs
                    .iter()
                    .enumerate()
                    .map(move |(ix, outlet)| (OutletId::new(node, ix), outlet))
            })
            .filter(|(_, o)| !o.fact.datum_type.is_concrete() || !o.fact.shape.is_concrete())
            .map(|(id, _)| id)
            .collect())
    }

    /// Eliminate seemingly dead branches of the graph.
    ///
    /// This may break stateful networks.
    pub fn eliminate_dead_branches(mut self) -> TractResult<InferenceModel> {
        compact::compact(&mut self)
    }

    /// Attempt full analyse and conversion to TypedModel.
    pub fn into_typed(mut self) -> TractResult<TypedModel> {
        self.analyse(false)?;
        let m = self.incorporate()?;

        #[derive(Debug)]
        struct ToTypedTranslator;
        impl Translate<InferenceFact, Box<dyn InferenceOp>, TypedFact, Box<dyn TypedOp>>
            for ToTypedTranslator
        {
            fn translate_node(
                &self,
                source: &InferenceModel,
                node: &InferenceNode,
                target: &mut TypedModel,
                mapping: &HashMap<OutletId, OutletId>,
            ) -> TractResult<TVec<OutletId>> {
                node.op.to_typed(source, node, target, mapping)
            }
        }

        ToTypedTranslator.translate_model(&m)
    }

    /// Attempt full analyse, decluttering and conversion to NormalizedModel.
    pub fn into_normalized(self) -> TractResult<NormalizedModel> {
        self.into_typed()?.declutter()?.into_normalized()
    }

    /// Attempt full analyse, decluttering and mapping to optimized operations.
    ///
    /// This will work even if the network can not be normalized.
    pub fn into_optimized(self) -> TractResult<TypedModel> {
        self.into_typed()?.into_optimized()
    }
}

impl ModelSpecialOps<InferenceFact, Box<dyn InferenceOp>> for InferenceModel {
    fn add_source(
        &mut self,
        name: impl Into<String>,
        fact: InferenceFact,
    ) -> TractResult<OutletId> {
        let id = self.add_node(name, crate::hir::ops::source::Source::new(), tvec!(fact))?;
        let id = OutletId::new(id, 0);
        self.inputs.push(id);
        Ok(id)
    }

    fn is_source(op: &dyn Op) -> bool {
        op.downcast_ref::<crate::hir::ops::source::Source>().is_some()
    }

    fn create_dummy(&self) -> Box<dyn InferenceOp> {
        Box::new(Dummy::new())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test() {
        fn is_sync<T: Sync>() {}
        is_sync::<InferenceModel>();
    }
}
