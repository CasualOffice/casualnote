//! One implementation of the seam, in pure Rust: ONNX through tract, tokens through `tokenizers`.
//! The same code runs natively and as the separately built browser module. The model file arrives
//! from the host as bytes. Nothing here downloads, and nothing is bundled.

use crate::{EmbedError, Embedder, Kind};
use std::io::Cursor;
use tokenizers::{Tokenizer, TruncationParams};
use tract_onnx::prelude::*;

/// How a model wants its inputs marked, from its model card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefix {
    /// `paraphrase-multilingual-MiniLM-L12-v2`: none.
    None,
    /// `multilingual-e5-small`: `query: ` and `passage: `.
    E5,
}

/// Lengths a plan is compiled for when the model will not take an open length. Shorter inputs pad
/// up to the next one. Compiling costs time, so it happens the first time a length is needed.
const BUCKETS: [usize; 4] = [16, 32, 64, 128];

/// A loaded model.
pub struct OnnxEmbedder {
    source: InferenceModel,
    open_plan: Option<Arc<TypedRunnableModel>>,
    plans: Vec<(usize, Arc<TypedRunnableModel>)>,
    open_error: String,
    tokenizer: Tokenizer,
    pad: i64,
    dims: usize,
    prefix: Prefix,
    inputs: Vec<String>,
}

impl std::fmt::Debug for OnnxEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OnnxEmbedder({} dims)", self.dims)
    }
}

fn fail(e: impl std::fmt::Display) -> EmbedError {
    EmbedError::Runtime(format!("{e:#}"))
}

impl OnnxEmbedder {
    /// Loads a model from an ONNX file and a `tokenizer.json`, both as bytes from the host.
    /// `max_tokens` is the model's input budget.
    pub fn from_bytes(
        model: &[u8],
        tokenizer: &[u8],
        dims: usize,
        prefix: Prefix,
        max_tokens: usize,
    ) -> Result<Self, EmbedError> {
        let mut tokenizer = Tokenizer::from_bytes(tokenizer).map_err(fail)?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: max_tokens,
                ..TruncationParams::default()
            }))
            .map_err(fail)?;
        tokenizer.with_padding(None);
        // The file names dimensions as free symbols, on outputs and inside the graph, and those do not
        // unify with a fixed input length. The analysis works them out again without them.
        let parsed = tract_onnx::onnx()
            .with_ignore_output_shapes(true)
            .with_ignore_value_info(true)
            .model_for_read(&mut Cursor::new(model))
            .map_err(fail)?;
        let inputs: Vec<String> = parsed
            .input_outlets()
            .map_err(fail)?
            .iter()
            .map(|o| parsed.node(o.node).name.clone())
            .collect();
        let pad = tokenizer.token_to_id("<pad>").map_or(1, i64::from);
        let mut me = Self {
            source: parsed,
            open_plan: None,
            plans: Vec::new(),
            open_error: String::new(),
            tokenizer,
            pad,
            dims,
            prefix,
            inputs,
        };
        match me.compile_open() {
            Ok(plan) => me.open_plan = Some(plan),
            Err(e) => me.open_error = e.to_string(),
        }
        Ok(me)
    }

    fn compile_open(&self) -> Result<Arc<TypedRunnableModel>, EmbedError> {
        let mut model = self.source.clone();
        let length = model.symbols.sym("S");
        for i in 0..self.inputs.len() {
            let shape: TVec<TDim> = tvec!(1.to_dim(), length.to_dim());
            model
                .set_input_fact(i, i64::fact(shape).into())
                .map_err(fail)?;
        }
        model
            .into_optimized()
            .map_err(fail)?
            .into_runnable()
            .map_err(fail)
    }

    fn plan_for(&mut self, n: usize) -> Result<(usize, Arc<TypedRunnableModel>), EmbedError> {
        if let Some(plan) = &self.open_plan {
            return Ok((n, plan.clone()));
        }
        let bucket = BUCKETS.iter().copied().find(|b| *b >= n).unwrap_or(n);
        if let Some((_, plan)) = self.plans.iter().find(|(b, _)| *b == bucket) {
            return Ok((bucket, plan.clone()));
        }
        let mut model = self.source.clone();
        for i in 0..self.inputs.len() {
            model
                .set_input_fact(i, i64::fact([1usize, bucket]).into())
                .map_err(fail)?;
        }
        let plan = model
            .into_optimized()
            .map_err(fail)?
            .into_runnable()
            .map_err(fail)?;
        self.plans.push((bucket, plan.clone()));
        Ok((bucket, plan))
    }

    /// Why an open input length was not possible, if it was not.
    pub fn open_length_error(&self) -> &str {
        &self.open_error
    }

    fn one(&mut self, text: &str, kind: Kind) -> Result<Vec<f32>, EmbedError> {
        let marked = match (self.prefix, kind) {
            (Prefix::None, _) => text.to_string(),
            (Prefix::E5, Kind::Query) => format!("query: {text}"),
            (Prefix::E5, Kind::Passage) => format!("passage: {text}"),
        };
        let encoding = self.tokenizer.encode(marked, true).map_err(fail)?;
        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|x| i64::from(*x)).collect();
        let mut mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|x| i64::from(*x))
            .collect();
        let (n, plan) = self.plan_for(ids.len())?;
        ids.resize(n, self.pad);
        mask.resize(n, 0);
        let types = vec![0i64; n];
        let mut feed: TVec<TValue> = tvec!();
        for name in &self.inputs {
            let source = if name.contains("mask") {
                &mask
            } else if name.contains("type") {
                &types
            } else {
                &ids
            };
            feed.push(Tensor::from_shape(&[1, n], source).map_err(fail)?.into());
        }
        let out = plan.run(feed).map_err(fail)?;
        let first = out.first().ok_or_else(|| fail("no output"))?;
        let view = first.to_plain_array_view::<f32>().map_err(fail)?;
        let shape = view.shape().to_vec();
        if shape.len() != 3 || shape[2] != self.dims || shape[1] != n {
            return Err(EmbedError::BadDimensions {
                expected: self.dims,
                got: shape.last().copied().unwrap_or(0),
            });
        }
        // Mean pooling over the mask, as both model cards say, then normalised.
        let data = view
            .as_slice()
            .ok_or_else(|| fail("output not contiguous"))?;
        let mut sum = vec![0.0f32; self.dims];
        let mut count = 0.0f32;
        for (i, row) in data.chunks_exact(self.dims).enumerate() {
            if mask.get(i).copied().unwrap_or(0) == 0 {
                continue;
            }
            count += 1.0;
            for (a, b) in sum.iter_mut().zip(row) {
                *a += b;
            }
        }
        if count > 0.0 {
            for x in &mut sum {
                *x /= count;
            }
        }
        crate::normalise(&mut sum);
        Ok(sum)
    }

    /// How many tokens an input becomes.
    pub fn token_count(&self, text: &str) -> usize {
        self.tokenizer
            .encode(text, true)
            .map(|e| e.get_ids().len())
            .unwrap_or(0)
    }
}

impl Embedder for OnnxEmbedder {
    fn dims(&self) -> usize {
        self.dims
    }

    fn embed(&mut self, texts: &[String], kind: Kind) -> Result<Vec<f32>, EmbedError> {
        let mut out = Vec::with_capacity(texts.len() * self.dims);
        for text in texts {
            out.extend(self.one(text, kind)?);
        }
        Ok(out)
    }
}
