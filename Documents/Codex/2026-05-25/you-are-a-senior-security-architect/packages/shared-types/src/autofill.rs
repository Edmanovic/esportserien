//! Secure autofill policy primitives shared across extension and desktop.

use serde::{Deserialize, Serialize};

/// Autofill decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AutofillDecision {
    /// Autofill may proceed.
    Allow,
    /// Autofill is blocked.
    Block,
    /// Autofill needs explicit user confirmation.
    RequireUserGesture,
}

/// Normalized origin validation input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OriginValidationRequest {
    /// Candidate frame origin.
    pub origin: String,
    /// Top-level page origin.
    pub top_level_origin: String,
    /// Saved item origin.
    pub saved_origin: String,
    /// Whether the field is visible and interactable.
    pub field_visible: bool,
    /// Whether the field is inside a cross-origin iframe.
    pub cross_origin_iframe: bool,
    /// Whether the action follows a user gesture.
    pub user_gesture: bool,
    /// Whether punycode or Unicode confusable risk was detected by extension.
    pub suspicious_domain: bool,
}

/// Secure autofill policy engine.
#[derive(Debug, Clone, Copy, Default)]
pub struct AutofillPolicyEngine;

impl AutofillPolicyEngine {
    /// Evaluates whether autofill is allowed.
    #[must_use]
    pub fn evaluate(request: &OriginValidationRequest) -> AutofillDecision {
        if request.suspicious_domain
            || !request.field_visible
            || request.cross_origin_iframe
            || request.origin != request.top_level_origin
            || request.origin != request.saved_origin
        {
            return AutofillDecision::Block;
        }
        if !request.user_gesture {
            return AutofillDecision::RequireUserGesture;
        }
        AutofillDecision::Allow
    }
}
