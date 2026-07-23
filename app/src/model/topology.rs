//! PCIe topology model for the Topology Tree pane.
//!
//! The interesting diagnostic here is not the device list — every OS gives you
//! that — but *link negotiation*: a x16 Gen5 card sitting in a slot that
//! negotiated x4 Gen3 is a real, common, silent performance bug. Carrying both
//! the current and maximum link parameters lets the UI flag degradation.
//!
//! Backends: `/sys/bus/pci/devices/*` on Linux, SetupAPI + config space on
//! Windows, IORegistry on macOS. Enumerated on the slow lane — topology only
//! changes on hotplug.

// A transcription of the PCIe topology surface, carried in full so a backend
// can be added without first extending the model. Exercised by the tests below
// until the M2 Topology pane and M3 backends land.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Role of a node in the PCIe hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Synthetic root of the tree (the machine).
    Root,
    /// A PCI segment/domain.
    Domain,
    HostBridge,
    RootPort,
    Switch,
    Bridge,
    Endpoint,
}

/// Negotiated or maximum link parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinkState {
    /// Transfer rate per lane in GT/s (2.5 = Gen1, 32 = Gen5).
    pub speed_gts: f32,
    pub width: u8,
}

impl LinkState {
    /// PCIe generation implied by the per-lane rate.
    pub fn generation(&self) -> u8 {
        match self.speed_gts {
            g if g >= 64.0 => 6,
            g if g >= 32.0 => 5,
            g if g >= 16.0 => 4,
            g if g >= 8.0 => 3,
            g if g >= 5.0 => 2,
            _ => 1,
        }
    }

    /// Approximate usable bandwidth in GB/s, accounting for line coding
    /// (8b/10b below Gen3, 128b/130b at Gen3+).
    pub fn bandwidth_gbps(&self) -> f32 {
        let efficiency = if self.generation() >= 3 { 128.0 / 130.0 } else { 0.8 };
        self.speed_gts * self.width as f32 * efficiency / 8.0
    }
}

/// One node in the PCIe tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PciNode {
    /// Bus:Device.Function address, e.g. `0000:01:00.0`.
    pub bdf: String,
    pub kind: NodeKind,
    /// Resolved vendor name, falling back to the hex id when unknown.
    pub vendor: String,
    pub device: String,
    pub vendor_id: u16,
    pub device_id: u16,
    /// PCI class code, for grouping and iconography.
    pub class_code: u8,
    pub subclass: u8,
    /// Bound kernel driver, when the OS exposes it.
    pub driver: Option<String>,
    /// Currently negotiated link. `None` for nodes without a link (e.g. Root).
    pub link: Option<LinkState>,
    /// Link the device is capable of.
    pub max_link: Option<LinkState>,
    pub children: Vec<PciNode>,
}

impl PciNode {
    /// True when the link negotiated below the device's capability — the
    /// headline diagnostic this pane exists to surface.
    pub fn link_degraded(&self) -> bool {
        match (self.link, self.max_link) {
            (Some(cur), Some(max)) => cur.width < max.width || cur.speed_gts < max.speed_gts,
            _ => false,
        }
    }

    /// One-line summary for the tree row, e.g. `x8 Gen4 (of x16 Gen4)`.
    pub fn link_summary(&self) -> Option<String> {
        let cur = self.link?;
        let base = format!("x{} Gen{}", cur.width, cur.generation());
        match self.max_link {
            Some(max) if self.link_degraded() => {
                Some(format!("{base} (of x{} Gen{})", max.width, max.generation()))
            }
            _ => Some(base),
        }
    }

    /// Depth-first iteration over this node and all descendants.
    pub fn walk(&self, f: &mut impl FnMut(&PciNode)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    /// Every degraded link in this subtree.
    pub fn degraded_links(&self) -> Vec<&PciNode> {
        let mut out = Vec::new();
        // `walk` takes &mut impl FnMut, so collect through a raw loop to keep
        // the borrow checker happy about pushing references.
        fn visit<'a>(n: &'a PciNode, out: &mut Vec<&'a PciNode>) {
            if n.link_degraded() {
                out.push(n);
            }
            for c in &n.children {
                visit(c, out);
            }
        }
        visit(self, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(bdf: &str, link: Option<LinkState>, max: Option<LinkState>) -> PciNode {
        PciNode {
            bdf: bdf.into(),
            kind: NodeKind::Endpoint,
            vendor: "Test".into(),
            device: "Device".into(),
            vendor_id: 0x1002,
            device_id: 0x744c,
            class_code: 0x03,
            subclass: 0x00,
            driver: None,
            link,
            max_link: max,
            children: vec![],
        }
    }

    const GEN4_X16: LinkState = LinkState { speed_gts: 16.0, width: 16 };
    const GEN4_X8: LinkState = LinkState { speed_gts: 16.0, width: 8 };
    const GEN1_X16: LinkState = LinkState { speed_gts: 2.5, width: 16 };

    #[test]
    fn generation_derives_from_per_lane_rate() {
        assert_eq!(GEN1_X16.generation(), 1);
        assert_eq!(LinkState { speed_gts: 5.0, width: 4 }.generation(), 2);
        assert_eq!(LinkState { speed_gts: 8.0, width: 4 }.generation(), 3);
        assert_eq!(GEN4_X16.generation(), 4);
        assert_eq!(LinkState { speed_gts: 32.0, width: 4 }.generation(), 5);
    }

    #[test]
    fn bandwidth_accounts_for_line_coding() {
        // Gen4 x16 ≈ 31.5 GB/s with 128b/130b encoding.
        assert!((GEN4_X16.bandwidth_gbps() - 31.5).abs() < 0.2);
        // Gen1 x16 ≈ 4 GB/s with 8b/10b.
        assert!((GEN1_X16.bandwidth_gbps() - 4.0).abs() < 0.01);
    }

    #[test]
    fn detects_width_and_speed_degradation() {
        assert!(endpoint("0000:01:00.0", Some(GEN4_X8), Some(GEN4_X16)).link_degraded());
        assert!(endpoint("0000:01:00.0", Some(GEN1_X16), Some(GEN4_X16)).link_degraded());
        assert!(!endpoint("0000:01:00.0", Some(GEN4_X16), Some(GEN4_X16)).link_degraded());
    }

    #[test]
    fn unknown_capability_is_not_reported_as_degraded() {
        // A missing max_link means "we couldn't read it", not "it's fine to
        // accuse the slot of being slow".
        assert!(!endpoint("0000:01:00.0", Some(GEN4_X8), None).link_degraded());
        assert!(!endpoint("0000:00:00.0", None, None).link_degraded());
    }

    #[test]
    fn summary_names_the_capability_only_when_degraded() {
        let ok = endpoint("0000:01:00.0", Some(GEN4_X16), Some(GEN4_X16));
        assert_eq!(ok.link_summary().as_deref(), Some("x16 Gen4"));
        let bad = endpoint("0000:01:00.0", Some(GEN4_X8), Some(GEN4_X16));
        assert_eq!(bad.link_summary().as_deref(), Some("x8 Gen4 (of x16 Gen4)"));
    }

    #[test]
    fn finds_degraded_links_anywhere_in_the_subtree() {
        let mut root = endpoint("0000:00:00.0", None, None);
        root.kind = NodeKind::Root;
        let mut port = endpoint("0000:00:01.0", Some(GEN4_X16), Some(GEN4_X16));
        port.kind = NodeKind::RootPort;
        port.children.push(endpoint("0000:01:00.0", Some(GEN4_X8), Some(GEN4_X16)));
        root.children.push(port);

        let degraded = root.degraded_links();
        assert_eq!(degraded.len(), 1);
        assert_eq!(degraded[0].bdf, "0000:01:00.0");

        let mut count = 0;
        root.walk(&mut |_| count += 1);
        assert_eq!(count, 3);
    }
}
