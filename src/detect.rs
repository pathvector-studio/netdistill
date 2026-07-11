#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    BgpSummary,
    RouteTable,
    Interfaces,
    Config,
    Generic,
}

/// Best-effort classification of a chunk of CLI output.
pub fn detect(input: &str) -> Kind {
    let head: Vec<&str> = input.lines().take(200).collect();
    let has = |needle: &str| head.iter().any(|l| l.contains(needle));

    if has("line protocol is") {
        return Kind::Interfaces;
    }
    if has("MsgRcvd")
        || has("State/PfxRcd")
        || (has("InPkt") && has("OutPkt"))
        || has("BGP router identifier")
    {
        return Kind::BgpSummary;
    }
    if has("Routing Table:")
        || has("Gateway of last resort")
        || has("variably subnetted")
        || head.iter().any(|l| l.trim_start().starts_with("Codes:"))
        || head.iter().any(|l| junos_rib_header(l).is_some())
    {
        return Kind::RouteTable;
    }
    let bangs = input.lines().filter(|l| l.trim() == "!").count();
    let has_iface_block = input.lines().any(|l| l.starts_with("interface "));
    if has_iface_block && (bangs >= 3 || input.lines().any(|l| l.starts_with("hostname "))) {
        return Kind::Config;
    }
    Kind::Generic
}

/// Junos RIB header: "inet.0: 25 destinations, 26 routes (25 active, 0 holddown, 1 hidden)"
pub fn junos_rib_header(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    let (name, rest) = t.split_once(':')?;
    if name.contains("inet") && !name.contains(' ') && rest.contains("destinations") {
        Some((name.to_string(), rest.trim().to_string()))
    } else {
        None
    }
}
