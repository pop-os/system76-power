// Copyright 2018-2021 System76 <info@system76.com>
//
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum GraphicsMode {
    Integrated,
    Compute,
    Hybrid,
    Discrete,
}

impl From<GraphicsMode> for &'static str {
    fn from(mode: GraphicsMode) -> &'static str {
        match mode {
            GraphicsMode::Integrated => "integrated",
            GraphicsMode::Compute => "compute",
            GraphicsMode::Hybrid => "hybrid",
            GraphicsMode::Discrete => "nvidia",
        }
    }
}

impl From<&str> for GraphicsMode {
    fn from(vendor: &str) -> Self {
        match vendor {
            "nvidia" => GraphicsMode::Discrete,
            "hybrid" => GraphicsMode::Hybrid,
            "compute" => GraphicsMode::Compute,
            _ => GraphicsMode::Integrated,
        }
    }
}
