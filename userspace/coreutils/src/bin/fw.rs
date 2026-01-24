//! fw - Firewall management utility for OXIDE
//!
//! Manages packet filter rules:
//! - fw add <chain> [-p proto] [--sport port] [--dport port] [-s ip] [-d ip] [-m state --state <state>] -j <action>
//! - fw del <index>
//! - fw list
//! - fw policy <chain> <action>
//! - fw flush [chain]
//! - fw save
//! - fw restore
//! - fw conntrack

#![no_std]
#![no_main]

use libc::syscall::{
    ConntrackStats, FwRule, fw_action, fw_chain, fw_proto, fw_state, sys_fw_add_rule,
    sys_fw_del_rule, sys_fw_flush, sys_fw_get_conntrack, sys_fw_list_rules, sys_fw_set_policy,
};
use libc::*;

fn print(s: &str) {
    syscall::sys_write(1, s.as_bytes());
}

fn print_num(n: u64) {
    if n == 0 {
        print("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    let mut num = n;
    while num > 0 {
        i -= 1;
        buf[i] = b'0' + (num % 10) as u8;
        num /= 10;
    }
    if let Ok(s) = core::str::from_utf8(&buf[i..]) {
        print(s);
    }
}

fn print_i32(n: i32) {
    if n < 0 {
        print("-");
        print_num((-n) as u64);
    } else {
        print_num(n as u64);
    }
}

fn cstr_to_str(ptr: *const u8) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
    }
}

/// Parse IP address from string (e.g., "10.0.2.2" or "10.0.2.0/24")
/// Returns (ip as u32 in network byte order, prefix_len)
fn parse_ip(s: &str) -> Option<(u32, u8)> {
    // Check for CIDR notation
    let (ip_part, prefix) = if let Some(slash_pos) = s.find('/') {
        let prefix_str = &s[slash_pos + 1..];
        let prefix: u8 = parse_u8(prefix_str)?;
        if prefix > 32 {
            return None;
        }
        (&s[..slash_pos], prefix)
    } else {
        (s, 32)
    };

    let mut octets = [0u8; 4];
    let mut octet_idx = 0;
    let mut current: u16 = 0;
    let mut has_digit = false;

    for c in ip_part.bytes() {
        if c == b'.' {
            if !has_digit || octet_idx >= 3 || current > 255 {
                return None;
            }
            octets[octet_idx] = current as u8;
            octet_idx += 1;
            current = 0;
            has_digit = false;
        } else if c >= b'0' && c <= b'9' {
            current = current * 10 + (c - b'0') as u16;
            has_digit = true;
            if current > 255 {
                return None;
            }
        } else {
            return None;
        }
    }

    if !has_digit || octet_idx != 3 || current > 255 {
        return None;
    }
    octets[octet_idx] = current as u8;

    let ip = u32::from_be_bytes(octets);
    Some((ip, prefix))
}

fn parse_u8(s: &str) -> Option<u8> {
    let mut val: u16 = 0;
    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            val = val * 10 + (c - b'0') as u16;
            if val > 255 {
                return None;
            }
        } else {
            return None;
        }
    }
    Some(val as u8)
}

fn parse_u16(s: &str) -> Option<u16> {
    let mut val: u32 = 0;
    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            val = val * 10 + (c - b'0') as u32;
            if val > 65535 {
                return None;
            }
        } else {
            return None;
        }
    }
    Some(val as u16)
}

fn parse_usize(s: &str) -> Option<usize> {
    let mut val: usize = 0;
    for c in s.bytes() {
        if c >= b'0' && c <= b'9' {
            val = val.checked_mul(10)?.checked_add((c - b'0') as usize)?;
        } else {
            return None;
        }
    }
    Some(val)
}

fn parse_chain(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase_manual() {
        "input" => Some(fw_chain::INPUT),
        "output" => Some(fw_chain::OUTPUT),
        "forward" => Some(fw_chain::FORWARD),
        _ => None,
    }
}

fn parse_action(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase_manual() {
        "accept" => Some(fw_action::ACCEPT),
        "drop" => Some(fw_action::DROP),
        "reject" => Some(fw_action::REJECT),
        _ => None,
    }
}

fn parse_proto(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase_manual() {
        "any" | "all" => Some(fw_proto::ANY),
        "icmp" => Some(fw_proto::ICMP),
        "tcp" => Some(fw_proto::TCP),
        "udp" => Some(fw_proto::UDP),
        _ => parse_u8(s),
    }
}

fn parse_state(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase_manual() {
        "new" => Some(fw_state::NEW),
        "established" => Some(fw_state::ESTABLISHED),
        "related" => Some(fw_state::RELATED),
        "invalid" => Some(fw_state::INVALID),
        _ => None,
    }
}

trait ToAsciiLowercaseManual {
    fn to_ascii_lowercase_manual(&self) -> &str;
}

impl ToAsciiLowercaseManual for &str {
    fn to_ascii_lowercase_manual(&self) -> &str {
        // Just compare case-insensitively by checking each pattern
        *self
    }
}

// Case-insensitive string comparison
fn str_eq_ignore_case(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (ca, cb) in a.bytes().zip(b.bytes()) {
        let la = if ca >= b'A' && ca <= b'Z' {
            ca + 32
        } else {
            ca
        };
        let lb = if cb >= b'A' && cb <= b'Z' {
            cb + 32
        } else {
            cb
        };
        if la != lb {
            return false;
        }
    }
    true
}

fn chain_name(chain: u8) -> &'static str {
    match chain {
        0 => "input",
        1 => "output",
        2 => "forward",
        _ => "unknown",
    }
}

fn action_name(action: u8) -> &'static str {
    match action {
        0 => "accept",
        1 => "drop",
        2 => "reject",
        _ => "unknown",
    }
}

fn proto_name(proto: u8) -> &'static str {
    match proto {
        0 => "all",
        1 => "icmp",
        6 => "tcp",
        17 => "udp",
        _ => "?",
    }
}

fn state_name(state: u8) -> &'static str {
    match state {
        0 => "",
        1 => "new",
        2 => "established",
        3 => "related",
        4 => "invalid",
        _ => "?",
    }
}

fn print_ip(ip: u32, prefix: u8) {
    let bytes = ip.to_be_bytes();
    print_num(bytes[0] as u64);
    print(".");
    print_num(bytes[1] as u64);
    print(".");
    print_num(bytes[2] as u64);
    print(".");
    print_num(bytes[3] as u64);
    if prefix < 32 {
        print("/");
        print_num(prefix as u64);
    }
}

fn cmd_add(args: &[&str]) {
    if args.len() < 2 {
        print(
            "Usage: fw add <chain> [-p proto] [--sport port] [--dport port] [-s ip[/prefix]] [-d ip[/prefix]] [-m state --state <state>] -j <action>\n",
        );
        return;
    }

    let chain = match args[0] {
        s if str_eq_ignore_case(s, "input") => fw_chain::INPUT,
        s if str_eq_ignore_case(s, "output") => fw_chain::OUTPUT,
        s if str_eq_ignore_case(s, "forward") => fw_chain::FORWARD,
        _ => {
            print("Invalid chain. Use: input, output, forward\n");
            return;
        }
    };

    let mut rule = FwRule::default();
    rule.chain = chain;

    let mut i = 1;
    let mut action_set = false;

    while i < args.len() {
        let arg = args[i];
        match arg {
            "-p" | "--protocol" => {
                i += 1;
                if i >= args.len() {
                    print("Missing protocol argument\n");
                    return;
                }
                match args[i] {
                    s if str_eq_ignore_case(s, "tcp") => rule.protocol = fw_proto::TCP,
                    s if str_eq_ignore_case(s, "udp") => rule.protocol = fw_proto::UDP,
                    s if str_eq_ignore_case(s, "icmp") => rule.protocol = fw_proto::ICMP,
                    s if str_eq_ignore_case(s, "all") || str_eq_ignore_case(s, "any") => {
                        rule.protocol = fw_proto::ANY
                    }
                    s => {
                        if let Some(p) = parse_u8(s) {
                            rule.protocol = p;
                        } else {
                            print("Invalid protocol\n");
                            return;
                        }
                    }
                }
            }
            "--sport" => {
                i += 1;
                if i >= args.len() {
                    print("Missing source port argument\n");
                    return;
                }
                if let Some(port) = parse_u16(args[i]) {
                    rule.src_port_start = port;
                    rule.src_port_end = port;
                } else {
                    print("Invalid source port\n");
                    return;
                }
            }
            "--dport" => {
                i += 1;
                if i >= args.len() {
                    print("Missing destination port argument\n");
                    return;
                }
                if let Some(port) = parse_u16(args[i]) {
                    rule.dst_port_start = port;
                    rule.dst_port_end = port;
                } else {
                    print("Invalid destination port\n");
                    return;
                }
            }
            "-s" | "--src" | "--source" => {
                i += 1;
                if i >= args.len() {
                    print("Missing source IP argument\n");
                    return;
                }
                if let Some((ip, prefix)) = parse_ip(args[i]) {
                    rule.src_ip = ip;
                    rule.src_prefix = prefix;
                } else {
                    print("Invalid source IP\n");
                    return;
                }
            }
            "-d" | "--dst" | "--destination" => {
                i += 1;
                if i >= args.len() {
                    print("Missing destination IP argument\n");
                    return;
                }
                if let Some((ip, prefix)) = parse_ip(args[i]) {
                    rule.dst_ip = ip;
                    rule.dst_prefix = prefix;
                } else {
                    print("Invalid destination IP\n");
                    return;
                }
            }
            "-m" => {
                i += 1;
                if i >= args.len() {
                    print("Missing match module argument\n");
                    return;
                }
                if str_eq_ignore_case(args[i], "state") {
                    // Need --state next
                    i += 1;
                    if i >= args.len() || !str_eq_ignore_case(args[i], "--state") {
                        print("Expected --state after -m state\n");
                        return;
                    }
                    i += 1;
                    if i >= args.len() {
                        print("Missing state argument\n");
                        return;
                    }
                    match args[i] {
                        s if str_eq_ignore_case(s, "new") => rule.state = fw_state::NEW,
                        s if str_eq_ignore_case(s, "established") => {
                            rule.state = fw_state::ESTABLISHED
                        }
                        s if str_eq_ignore_case(s, "related") => rule.state = fw_state::RELATED,
                        s if str_eq_ignore_case(s, "invalid") => rule.state = fw_state::INVALID,
                        _ => {
                            print("Invalid state. Use: new, established, related, invalid\n");
                            return;
                        }
                    }
                } else {
                    print("Unknown match module\n");
                    return;
                }
            }
            "-j" | "--jump" => {
                i += 1;
                if i >= args.len() {
                    print("Missing action argument\n");
                    return;
                }
                match args[i] {
                    s if str_eq_ignore_case(s, "accept") => rule.action = fw_action::ACCEPT,
                    s if str_eq_ignore_case(s, "drop") => rule.action = fw_action::DROP,
                    s if str_eq_ignore_case(s, "reject") => rule.action = fw_action::REJECT,
                    _ => {
                        print("Invalid action. Use: accept, drop, reject\n");
                        return;
                    }
                }
                action_set = true;
            }
            _ => {
                print("Unknown option: ");
                print(arg);
                print("\n");
                return;
            }
        }
        i += 1;
    }

    if !action_set {
        print("Error: action required (-j accept/drop/reject)\n");
        return;
    }

    let result = sys_fw_add_rule(&rule);
    if result < 0 {
        print("Failed to add rule: ");
        print_i32(result);
        print("\n");
    } else {
        print("Rule added\n");
    }
}

fn cmd_del(args: &[&str]) {
    if args.is_empty() {
        print("Usage: fw del <rule_index>\n");
        return;
    }

    let index = match parse_usize(args[0]) {
        Some(i) => i,
        None => {
            print("Invalid index\n");
            return;
        }
    };

    let result = sys_fw_del_rule(index);
    if result < 0 {
        print("Failed to delete rule: ");
        print_i32(result);
        print("\n");
    } else {
        print("Rule deleted\n");
    }
}

fn cmd_list() {
    // First get the count
    let count = sys_fw_list_rules(core::ptr::null_mut(), 0);
    if count < 0 {
        print("Failed to list rules: ");
        print_i32(count);
        print("\n");
        return;
    }

    if count == 0 {
        print("No rules\n");
        return;
    }

    // Allocate buffer on stack (max 64 rules)
    let max_rules = if count > 64 { 64 } else { count as usize };
    let mut rules = [FwRule::default(); 64];

    let result = sys_fw_list_rules(rules.as_mut_ptr(), max_rules);
    if result < 0 {
        print("Failed to list rules: ");
        print_i32(result);
        print("\n");
        return;
    }

    print(
        "Chain      Proto  Source                 Dest                   Ports              State        Action\n",
    );
    print(
        "---------- ------ ---------------------- ---------------------- ------------------ ------------ ------\n",
    );

    for i in 0..result as usize {
        let rule = &rules[i];

        // Index
        print_num(i as u64);
        print(") ");

        // Chain
        let chain = chain_name(rule.chain);
        print(chain);
        for _ in chain.len()..10 {
            print(" ");
        }
        print(" ");

        // Protocol
        let proto = proto_name(rule.protocol);
        print(proto);
        for _ in proto.len()..6 {
            print(" ");
        }
        print(" ");

        // Source IP
        if rule.src_prefix > 0 {
            print_ip(rule.src_ip, rule.src_prefix);
            // Pad to 22 chars
            let printed = if rule.src_prefix < 32 { 15 + 3 } else { 15 }; // ip + optional /prefix
            for _ in printed..22 {
                print(" ");
            }
        } else {
            print("any                   ");
        }
        print(" ");

        // Dest IP
        if rule.dst_prefix > 0 {
            print_ip(rule.dst_ip, rule.dst_prefix);
            let printed = if rule.dst_prefix < 32 { 15 + 3 } else { 15 };
            for _ in printed..22 {
                print(" ");
            }
        } else {
            print("any                   ");
        }
        print(" ");

        // Ports
        let mut port_str_len = 0;
        if rule.src_port_start > 0 || rule.dst_port_start > 0 {
            if rule.src_port_start > 0 {
                print_num(rule.src_port_start as u64);
                port_str_len += 5; // approx
            } else {
                print("*");
                port_str_len += 1;
            }
            print("->");
            port_str_len += 2;
            if rule.dst_port_start > 0 {
                print_num(rule.dst_port_start as u64);
                port_str_len += 5;
            } else {
                print("*");
                port_str_len += 1;
            }
            for _ in port_str_len..18 {
                print(" ");
            }
        } else {
            print("                  ");
        }
        print(" ");

        // State
        let state = state_name(rule.state);
        print(state);
        for _ in state.len()..12 {
            print(" ");
        }
        print(" ");

        // Action
        print(action_name(rule.action));
        print("\n");
    }
}

fn cmd_policy(args: &[&str]) {
    if args.len() < 2 {
        print("Usage: fw policy <chain> <accept|drop|reject>\n");
        return;
    }

    let chain = match args[0] {
        s if str_eq_ignore_case(s, "input") => fw_chain::INPUT,
        s if str_eq_ignore_case(s, "output") => fw_chain::OUTPUT,
        s if str_eq_ignore_case(s, "forward") => fw_chain::FORWARD,
        _ => {
            print("Invalid chain. Use: input, output, forward\n");
            return;
        }
    };

    let policy = match args[1] {
        s if str_eq_ignore_case(s, "accept") => fw_action::ACCEPT,
        s if str_eq_ignore_case(s, "drop") => fw_action::DROP,
        s if str_eq_ignore_case(s, "reject") => fw_action::REJECT,
        _ => {
            print("Invalid policy. Use: accept, drop, reject\n");
            return;
        }
    };

    let result = sys_fw_set_policy(chain, policy);
    if result < 0 {
        print("Failed to set policy: ");
        print_i32(result);
        print("\n");
    } else {
        print("Policy set\n");
    }
}

fn cmd_flush(args: &[&str]) {
    let chain = if args.is_empty() {
        fw_chain::ALL
    } else {
        match args[0] {
            s if str_eq_ignore_case(s, "input") => fw_chain::INPUT,
            s if str_eq_ignore_case(s, "output") => fw_chain::OUTPUT,
            s if str_eq_ignore_case(s, "forward") => fw_chain::FORWARD,
            s if str_eq_ignore_case(s, "all") => fw_chain::ALL,
            _ => {
                print("Invalid chain. Use: input, output, forward, all\n");
                return;
            }
        }
    };

    let result = sys_fw_flush(chain);
    if result < 0 {
        print("Failed to flush: ");
        print_i32(result);
        print("\n");
    } else {
        if chain == fw_chain::ALL {
            print("All rules flushed\n");
        } else {
            print("Chain flushed\n");
        }
    }
}

fn cmd_save() {
    // Output rules in a format that can be restored
    let count = sys_fw_list_rules(core::ptr::null_mut(), 0);
    if count < 0 {
        print("# Failed to list rules\n");
        return;
    }

    if count == 0 {
        print("# No rules to save\n");
        return;
    }

    let max_rules = if count > 64 { 64 } else { count as usize };
    let mut rules = [FwRule::default(); 64];

    let result = sys_fw_list_rules(rules.as_mut_ptr(), max_rules);
    if result < 0 {
        print("# Failed to list rules\n");
        return;
    }

    print("# OXIDE firewall rules\n");

    for i in 0..result as usize {
        let rule = &rules[i];

        print("add ");
        print(chain_name(rule.chain));

        if rule.protocol != 0 {
            print(" -p ");
            print(proto_name(rule.protocol));
        }

        if rule.src_prefix > 0 {
            print(" -s ");
            print_ip(rule.src_ip, rule.src_prefix);
        }

        if rule.dst_prefix > 0 {
            print(" -d ");
            print_ip(rule.dst_ip, rule.dst_prefix);
        }

        if rule.src_port_start > 0 {
            print(" --sport ");
            print_num(rule.src_port_start as u64);
        }

        if rule.dst_port_start > 0 {
            print(" --dport ");
            print_num(rule.dst_port_start as u64);
        }

        if rule.state != 0 {
            print(" -m state --state ");
            print(state_name(rule.state));
        }

        print(" -j ");
        print(action_name(rule.action));
        print("\n");
    }
}

fn cmd_restore(args: &[&str]) {
    if args.is_empty() {
        print("Usage: fw restore <filename>\n");
        return;
    }

    let filename = args[0];

    // Open the file
    let fd = syscall::sys_open(filename, 0, 0); // O_RDONLY
    if fd < 0 {
        print("Failed to open ");
        print(filename);
        print(": ");
        print_i32(fd);
        print("\n");
        return;
    }

    // Read file contents
    let mut buf = [0u8; 4096];
    let bytes_read = syscall::sys_read(fd, &mut buf);
    syscall::sys_close(fd);

    if bytes_read < 0 {
        print("Failed to read file: ");
        print_i32(bytes_read as i32);
        print("\n");
        return;
    }

    // Parse lines
    let content = match core::str::from_utf8(&buf[..bytes_read as usize]) {
        Ok(s) => s,
        Err(_) => {
            print("Invalid UTF-8 in rules file\n");
            return;
        }
    };

    let mut rules_added = 0;
    let mut errors = 0;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse the line as command arguments
        // Format: add <chain> [options] -j <action>
        let mut words: [&str; 32] = [""; 32];
        let mut word_count = 0;

        for word in line.split_whitespace() {
            if word_count < 32 {
                words[word_count] = word;
                word_count += 1;
            }
        }

        if word_count == 0 {
            continue;
        }

        // Check command
        match words[0] {
            "add" => {
                if word_count > 1 {
                    cmd_add_silent(&words[1..word_count], &mut rules_added, &mut errors);
                }
            }
            "policy" => {
                if word_count >= 3 {
                    cmd_policy_silent(&words[1..word_count]);
                }
            }
            "flush" => {
                cmd_flush(&words[1..word_count]);
            }
            _ => {
                // Ignore unknown commands in restore
            }
        }
    }

    print("Restored ");
    print_num(rules_added as u64);
    print(" rules");
    if errors > 0 {
        print(" (");
        print_num(errors as u64);
        print(" errors)");
    }
    print("\n");
}

// Silent version of cmd_add for restore (doesn't print success messages)
fn cmd_add_silent(args: &[&str], rules_added: &mut usize, errors: &mut usize) {
    if args.is_empty() {
        *errors += 1;
        return;
    }

    let chain = match args[0] {
        s if str_eq_ignore_case(s, "input") => fw_chain::INPUT,
        s if str_eq_ignore_case(s, "output") => fw_chain::OUTPUT,
        s if str_eq_ignore_case(s, "forward") => fw_chain::FORWARD,
        _ => {
            *errors += 1;
            return;
        }
    };

    let mut rule = FwRule::default();
    rule.chain = chain;

    let mut i = 1;
    let mut action_set = false;

    while i < args.len() {
        let arg = args[i];
        match arg {
            "-p" | "--protocol" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                match args[i] {
                    s if str_eq_ignore_case(s, "tcp") => rule.protocol = fw_proto::TCP,
                    s if str_eq_ignore_case(s, "udp") => rule.protocol = fw_proto::UDP,
                    s if str_eq_ignore_case(s, "icmp") => rule.protocol = fw_proto::ICMP,
                    s if str_eq_ignore_case(s, "all") || str_eq_ignore_case(s, "any") => {
                        rule.protocol = fw_proto::ANY
                    }
                    s => {
                        if let Some(p) = parse_u8(s) {
                            rule.protocol = p;
                        } else {
                            *errors += 1;
                            return;
                        }
                    }
                }
            }
            "--sport" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                if let Some(port) = parse_u16(args[i]) {
                    rule.src_port_start = port;
                    rule.src_port_end = port;
                } else {
                    *errors += 1;
                    return;
                }
            }
            "--dport" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                if let Some(port) = parse_u16(args[i]) {
                    rule.dst_port_start = port;
                    rule.dst_port_end = port;
                } else {
                    *errors += 1;
                    return;
                }
            }
            "-s" | "--src" | "--source" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                if let Some((ip, prefix)) = parse_ip(args[i]) {
                    rule.src_ip = ip;
                    rule.src_prefix = prefix;
                } else {
                    *errors += 1;
                    return;
                }
            }
            "-d" | "--dst" | "--destination" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                if let Some((ip, prefix)) = parse_ip(args[i]) {
                    rule.dst_ip = ip;
                    rule.dst_prefix = prefix;
                } else {
                    *errors += 1;
                    return;
                }
            }
            "-m" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                if str_eq_ignore_case(args[i], "state") {
                    i += 1;
                    if i >= args.len() || !str_eq_ignore_case(args[i], "--state") {
                        *errors += 1;
                        return;
                    }
                    i += 1;
                    if i >= args.len() {
                        *errors += 1;
                        return;
                    }
                    match args[i] {
                        s if str_eq_ignore_case(s, "new") => rule.state = fw_state::NEW,
                        s if str_eq_ignore_case(s, "established") => {
                            rule.state = fw_state::ESTABLISHED
                        }
                        s if str_eq_ignore_case(s, "related") => rule.state = fw_state::RELATED,
                        s if str_eq_ignore_case(s, "invalid") => rule.state = fw_state::INVALID,
                        _ => {
                            *errors += 1;
                            return;
                        }
                    }
                }
            }
            "-j" | "--jump" => {
                i += 1;
                if i >= args.len() {
                    *errors += 1;
                    return;
                }
                match args[i] {
                    s if str_eq_ignore_case(s, "accept") => rule.action = fw_action::ACCEPT,
                    s if str_eq_ignore_case(s, "drop") => rule.action = fw_action::DROP,
                    s if str_eq_ignore_case(s, "reject") => rule.action = fw_action::REJECT,
                    _ => {
                        *errors += 1;
                        return;
                    }
                }
                action_set = true;
            }
            _ => {}
        }
        i += 1;
    }

    if !action_set {
        *errors += 1;
        return;
    }

    let result = sys_fw_add_rule(&rule);
    if result < 0 {
        *errors += 1;
    } else {
        *rules_added += 1;
    }
}

// Silent version of cmd_policy for restore
fn cmd_policy_silent(args: &[&str]) {
    if args.len() < 2 {
        return;
    }

    let chain = match args[0] {
        s if str_eq_ignore_case(s, "input") => fw_chain::INPUT,
        s if str_eq_ignore_case(s, "output") => fw_chain::OUTPUT,
        s if str_eq_ignore_case(s, "forward") => fw_chain::FORWARD,
        _ => return,
    };

    let policy = match args[1] {
        s if str_eq_ignore_case(s, "accept") => fw_action::ACCEPT,
        s if str_eq_ignore_case(s, "drop") => fw_action::DROP,
        s if str_eq_ignore_case(s, "reject") => fw_action::REJECT,
        _ => return,
    };

    sys_fw_set_policy(chain, policy);
}

fn cmd_conntrack() {
    let mut stats = ConntrackStats { count: 0, max: 0 };
    let result = sys_fw_get_conntrack(&mut stats);

    if result < 0 {
        print("Failed to get conntrack info: ");
        print_i32(result);
        print("\n");
        return;
    }

    print("Connection tracking:\n");
    print("  Active connections: ");
    print_num(stats.count);
    print("\n");
    print("  Maximum connections: ");
    print_num(stats.max);
    print("\n");
}

fn print_usage() {
    print("Usage: fw <command> [options]\n\n");
    print("Commands:\n");
    print("  add <chain> [options] -j <action>  Add a rule\n");
    print("  del <index>                        Delete a rule by index\n");
    print("  list                               List all rules\n");
    print("  policy <chain> <action>            Set chain default policy\n");
    print("  flush [chain]                      Flush rules (all if no chain)\n");
    print("  save                               Output rules for restore\n");
    print("  restore <file>                     Load rules from file\n");
    print("  conntrack                          Show connection tracking info\n");
    print("\n");
    print("Chains: input, output, forward\n");
    print("Actions: accept, drop, reject\n");
    print("\n");
    print("Add options:\n");
    print("  -p <proto>          Protocol (tcp, udp, icmp, all)\n");
    print("  -s <ip[/prefix]>    Source IP/network\n");
    print("  -d <ip[/prefix]>    Destination IP/network\n");
    print("  --sport <port>      Source port\n");
    print("  --dport <port>      Destination port\n");
    print("  -m state --state <state>  Match connection state\n");
    print("                      (new, established, related, invalid)\n");
    print("\n");
    print("Examples:\n");
    print("  fw add input -p tcp --dport 22 -j accept\n");
    print("  fw add input -m state --state established -j accept\n");
    print("  fw add input -j drop\n");
    print("  fw policy input drop\n");
    print("  fw list\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    if argc < 2 {
        print_usage();
        return 1;
    }

    let args: &[*const u8] = unsafe { core::slice::from_raw_parts(argv, argc as usize) };
    let cmd = cstr_to_str(args[1]);

    // Convert remaining args to &str
    let mut cmd_args: [&str; 32] = [""; 32];
    let cmd_argc = if argc > 2 {
        let count = (argc - 2).min(32) as usize;
        for i in 0..count {
            cmd_args[i] = cstr_to_str(args[i + 2]);
        }
        count
    } else {
        0
    };

    match cmd {
        "add" => cmd_add(&cmd_args[..cmd_argc]),
        "del" | "delete" => cmd_del(&cmd_args[..cmd_argc]),
        "list" | "ls" | "-L" => cmd_list(),
        "policy" => cmd_policy(&cmd_args[..cmd_argc]),
        "flush" | "-F" => cmd_flush(&cmd_args[..cmd_argc]),
        "save" => cmd_save(),
        "restore" => cmd_restore(&cmd_args[..cmd_argc]),
        "conntrack" | "ct" => cmd_conntrack(),
        "-h" | "--help" | "help" => {
            print_usage();
        }
        _ => {
            print("Unknown command: ");
            print(cmd);
            print("\n");
            print_usage();
            return 1;
        }
    }

    0
}
