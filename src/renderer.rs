use serde_json::Value;
use std::collections::BTreeMap;

// Public entry points

/// Render a Swagger 2.0 spec (parsed by the openapi crate) to HTML.
pub fn render_swagger(spec: &openapi::Spec, title_override: Option<&str>) -> String {
    let title = title_override.unwrap_or(&spec.info.title);
    let version = &spec.info.version;

    let servers = swagger_servers(spec);
    let tags = spec.tags.as_deref().unwrap_or(&[]);

    // Group operations by tag
    let groups = swagger_groups(spec);

    // Build sidebar + content
    let sidebar = sidebar_html(title, version, &groups, |path, method, _op| {
        op_anchor(method, path)
    });

    let mut content = format!(
        r#"<header class="api-header">
  <h1>{title_esc}</h1>
  <div class="api-meta">
    <span class="ver-badge">v{ver}</span>
    <span class="oa-badge">Swagger {sw}</span>
  </div>"#,
        title_esc = esc(title),
        ver = esc(version),
        sw = esc(&spec.swagger),
    );
    if let Some(tos) = &spec.info.terms_of_service {
        content.push_str(&format!(
            r#"<div class="api-links"><a href="{}" class="api-link">Terms of Service</a></div>"#,
            esc(tos)
        ));
    }
    content.push_str("</header>");

    // Servers
    if !servers.is_empty() {
        content.push_str("<section class=\"info-section\"><h2>Servers</h2><ul class=\"server-list\">");
        for s in &servers {
            content.push_str(&format!("<li><code>{}</code></li>", esc(s)));
        }
        content.push_str("</ul></section>");
    }

    // Security definitions
    if let Some(sec_defs) = &spec.security_definitions {
        if !sec_defs.is_empty() {
            content.push_str("<section class=\"info-section\"><h2>Authentication</h2>");
            for (name, sec) in sec_defs {
                content.push_str(&format!(
                    r#"<div class="scheme-card"><span class="scheme-name">{}</span> <span class="scheme-type">{}</span></div>"#,
                    esc(name),
                    esc(&sec.security_type),
                ));
            }
            content.push_str("</section>");
        }
    }

    // Operations by tag
    for (tag, ops) in &groups {
        let tag_desc = tags
            .iter()
            .find(|t| &t.name == tag)
            .and_then(|t| t.description.as_deref())
            .map(|d| format!("<p class=\"tag-desc\">{}</p>", esc(d)))
            .unwrap_or_default();

        content.push_str(&format!(
            "<section class=\"tag-section\" id=\"tag-{}\"><h2 class=\"tag-heading\">{}</h2>{tag_desc}",
            slug(tag),
            esc(tag),
        ));
        for (path, method, op) in ops {
            content.push_str(&render_swagger_op(path, method, op));
        }
        content.push_str("</section>");
    }

    // Definitions (schemas)
    if !spec.definitions.is_empty() {
        content.push_str("<section class=\"info-section\" id=\"schemas\"><h2>Schemas</h2>");
        for (name, schema) in &spec.definitions {
            if let Ok(v) = serde_json::to_value(schema) {
                content.push_str(&format!(
                    r##"<div class="schema-card" id="schema-{slug}">
  <div class="schema-card-title">{name}</div>
  <pre class="code-block">{body}</pre>
</div>"##,
                    slug = slug(name),
                    name = esc(name),
                    body = esc(&pretty(&v)),
                ));
            }
        }
        content.push_str("</section>");
    }

    page(title, &sidebar, &content)
}

/// Render an OpenAPI 3.x spec parsed as a raw JSON Value.
pub fn render_oa3(value: &Value, title_override: Option<&str>) -> String {
    let info = value.get("info").unwrap_or(&Value::Null);
    let def_title = sv(info, "title").unwrap_or_else(|| "API Documentation".into());
    let title = title_override.unwrap_or(&def_title);
    let version = sv(info, "version").unwrap_or_else(|| "1.0".into());
    let oa_ver = sv(value, "openapi").unwrap_or_else(|| "3.x".into());

    let groups = oa3_groups(value);

    let sidebar = sidebar_html(title, &version, &groups, |path, method, _v| {
        op_anchor(method, path)
    });

    let mut content = format!(
        r#"<header class="api-header">
  <h1>{title_esc}</h1>
  <div class="api-meta">
    <span class="ver-badge">v{ver}</span>
    <span class="oa-badge">OpenAPI {oa}</span>
  </div>"#,
        title_esc = esc(title),
        ver = esc(&version),
        oa = esc(&oa_ver),
    );

    if let Some(desc) = sv(info, "description") {
        content.push_str(&format!("<p class=\"api-desc\">{}</p>", simple_md(&desc)));
    }

    let mut links = String::new();
    if let Some(tos) = sv(info, "termsOfService") {
        links.push_str(&format!("<a href=\"{}\" class=\"api-link\">Terms of Service</a>", esc(&tos)));
    }
    if let Some(c) = info.get("contact") {
        let href = sv(c, "email")
            .map(|e| format!("mailto:{e}"))
            .or_else(|| sv(c, "url"))
            .unwrap_or_default();
        let label = sv(c, "name").or_else(|| sv(c, "email")).unwrap_or_else(|| "Contact".into());
        if !href.is_empty() {
            links.push_str(&format!("<a href=\"{}\" class=\"api-link\">{}</a>", esc(&href), esc(&label)));
        }
    }
    if let Some(l) = info.get("license") {
        let name = sv(l, "name").unwrap_or_else(|| "License".into());
        let href = sv(l, "url").unwrap_or_else(|| "#".into());
        links.push_str(&format!("<a href=\"{}\" class=\"api-link\">{}</a>", esc(&href), esc(&name)));
    }
    if !links.is_empty() {
        content.push_str(&format!("<div class=\"api-links\">{links}</div>"));
    }
    content.push_str("</header>");

    // Servers
    if let Some(servers) = value.get("servers").and_then(|s| s.as_array()) {
        if !servers.is_empty() {
            content.push_str("<section class=\"info-section\"><h2>Servers</h2><ul class=\"server-list\">");
            for s in servers {
                let url = sv(s, "url").unwrap_or_default();
                let desc = sv(s, "description")
                    .map(|d| format!(" <span class=\"server-desc\">— {}</span>", esc(&d)))
                    .unwrap_or_default();
                content.push_str(&format!("<li><code>{}</code>{desc}</li>", esc(&url)));
            }
            content.push_str("</ul></section>");
        }
    }

    // Security schemes
    if let Some(schemes) = value
        .pointer("/components/securitySchemes")
        .and_then(|s| s.as_object())
    {
        if !schemes.is_empty() {
            content.push_str("<section class=\"info-section\"><h2>Authentication</h2>");
            for (name, scheme) in schemes {
                let stype = sv(scheme, "type").unwrap_or_else(|| "unknown".into());
                let desc = sv(scheme, "description")
                    .map(|d| format!("<p class=\"tag-desc\">{}</p>", esc(&d)))
                    .unwrap_or_default();
                content.push_str(&format!(
                    r#"<div class="scheme-card"><span class="scheme-name">{}</span> <span class="scheme-type">{}</span>{desc}</div>"#,
                    esc(name), esc(&stype),
                ));
            }
            content.push_str("</section>");
        }
    }

    // Operations by tag
    let tag_descs: BTreeMap<String, String> = value
        .get("tags")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
        .filter_map(|t| Some((sv(t, "name")?, sv(t, "description").unwrap_or_default())))
        .collect();

    for (tag, ops) in &groups {
        let tag_desc = tag_descs
            .get(tag.as_str())
            .filter(|d| !d.is_empty())
            .map(|d| format!("<p class=\"tag-desc\">{}</p>", esc(d)))
            .unwrap_or_default();

        content.push_str(&format!(
            "<section class=\"tag-section\" id=\"tag-{}\"><h2 class=\"tag-heading\">{}</h2>{tag_desc}",
            slug(tag),
            esc(tag),
        ));
        for (path, method, op_val) in ops {
            content.push_str(&render_oa3_op(path, method, op_val));
        }
        content.push_str("</section>");
    }

    // Schemas
    if let Some(schemas) = value
        .pointer("/components/schemas")
        .and_then(|s| s.as_object())
    {
        if !schemas.is_empty() {
            content.push_str("<section class=\"info-section\" id=\"schemas\"><h2>Schemas</h2>");
            for (name, schema) in schemas {
                content.push_str(&format!(
                    r##"<div class="schema-card" id="schema-{slug}">
  <div class="schema-card-title">{name}</div>
  <pre class="code-block">{body}</pre>
</div>"##,
                    slug = slug(name),
                    name = esc(name),
                    body = esc(&pretty(schema)),
                ));
            }
            content.push_str("</section>");
        }
    }

    page(title, &sidebar, &content)
}

// Swagger 2.0 rendering helpers
type SwaggerGroup<'a> = Vec<(String, Vec<(&'a str, &'a str, &'a openapi::Operation)>)>;

fn swagger_groups(spec: &openapi::Spec) -> SwaggerGroup<'_> {
    let mut groups: SwaggerGroup<'_> = Vec::new();

    for (path, ops) in &spec.paths {
        let method_ops: [(&str, Option<&openapi::Operation>); 5] = [
            ("GET", ops.get.as_ref()),
            ("POST", ops.post.as_ref()),
            ("PUT", ops.put.as_ref()),
            ("PATCH", ops.patch.as_ref()),
            ("DELETE", ops.delete.as_ref()),
        ];
        for (method, op_opt) in method_ops {
            if let Some(op) = op_opt {
                let tags = op.tags.as_deref().unwrap_or(&[]);
                let tag_list: Vec<&str> = if tags.is_empty() { vec!["default"] } else { tags.iter().map(|s| s.as_str()).collect() };
                for tag in tag_list {
                    match groups.iter_mut().find(|(t, _)| t == tag) {
                        Some(g) => g.1.push((path.as_str(), method, op)),
                        None => groups.push((tag.to_string(), vec![(path.as_str(), method, op)])),
                    }
                }
            }
        }
    }
    groups
}

fn render_swagger_op(path: &str, method: &str, op: &openapi::Operation) -> String {
    let anchor = op_anchor(method, path);
    let summary = op.summary.as_deref().unwrap_or("");

    let mut body = String::new();

    if let Some(desc) = &op.description {
        body.push_str(&format!("<p class=\"op-desc\">{}</p>", simple_md(desc)));
    }
    if let Some(oid) = &op.operation_id {
        body.push_str(&format!(
            "<div class=\"op-id\">operationId: <code>{}</code></div>",
            esc(oid)
        ));
    }

    // Parameters
    let params: Vec<&openapi::Parameter> = op.parameters.as_deref().unwrap_or(&[]).iter().collect();
    if !params.is_empty() {
        body.push_str(
            "<div class=\"params-section\"><h4 class=\"section-label\">Parameters</h4>\
             <table class=\"params-table\">\
             <thead><tr><th>Name</th><th>In</th><th>Type</th><th>Req</th><th>Description</th></tr></thead>\
             <tbody>",
        );
        for p in &params {
            if p.location == "body" {
                continue; // rendered as request body below
            }
            let type_str = p
                .param_type
                .as_deref()
                .or_else(|| p.schema.as_ref().and_then(|s| s.schema_type.as_deref()))
                .unwrap_or("any");
            let req = p.required.unwrap_or(p.location == "path");
            let req_cls = if req { " row-required" } else { "" };
            body.push_str(&format!(
                r#"<tr class="{req_cls}"><td><code>{name}</code></td><td><span class="param-in param-in-{loc_lc}">{loc}</span></td><td><code class="type-str">{type_str}</code></td><td>{req_mark}</td><td></td></tr>"#,
                name = esc(&p.name),
                loc = esc(&p.location),
                loc_lc = p.location.to_lowercase(),
                req_mark = if req { "✓" } else { "" },
            ));
        }
        body.push_str("</tbody></table></div>");
    }

    // Body parameter → request body
    if let Some(body_param) = params.iter().find(|p| p.location == "body") {
        body.push_str("<div class=\"body-section\"><h4 class=\"section-label\">Request Body</h4>");
        if let Some(schema) = &body_param.schema {
            if let Ok(v) = serde_json::to_value(schema) {
                body.push_str(&format!(
                    "<span class=\"content-type\">application/json</span><pre class=\"code-block\">{}</pre>",
                    esc(&pretty(&v))
                ));
            }
        }
        body.push_str("</div>");
    }

    // Responses
    if !op.responses.is_empty() {
        body.push_str("<div class=\"resp-section\"><h4 class=\"section-label\">Responses</h4>");
        for (code, resp) in &op.responses {
            let cls = status_class(code);
            body.push_str(&format!(
                r#"<div class="resp-item"><div class="resp-header"><span class="status-code status-{cls}">{code}</span> <span class="resp-desc">{desc}</span></div>"#,
                code = esc(code),
                desc = esc(&resp.description),
            ));
            if let Some(schema) = &resp.schema {
                if let Ok(v) = serde_json::to_value(schema) {
                    body.push_str(&format!(
                        "<pre class=\"code-block resp-schema\">{}</pre>",
                        esc(&pretty(&v))
                    ));
                }
            }
            body.push_str("</div>");
        }
        body.push_str("</div>");
    }

    format!(
        r##"<div class="operation" id="{anchor}">
  <div class="op-header" onclick="toggleOp('{anchor}')">
    <span class="badge badge-{ml}">{method}</span>
    <code class="op-path">{path_esc}</code>
    <span class="op-summary">{summary}</span>
    <span class="chevron">▾</span>
  </div>
  <div class="op-body" id="body-{anchor}">{body}</div>
</div>"##,
        ml = method.to_lowercase(),
        path_esc = esc(path),
        summary = esc(summary),
    )
}

fn swagger_servers(spec: &openapi::Spec) -> Vec<String> {
    let host = spec.host.as_deref().unwrap_or("localhost");
    let base = spec.base_path.as_deref().unwrap_or("/");
    let default_schemes = vec!["https".to_string()];
    let schemes = spec.schemes.as_deref().unwrap_or(&default_schemes);
    schemes.iter().map(|s| format!("{s}://{host}{base}")).collect()
}

// OpenAPI 3.x rendering helpers
type Oa3Group<'a> = Vec<(String, Vec<(&'a str, &'a str, &'a Value)>)>;

fn oa3_groups(value: &Value) -> Oa3Group<'_> {
    let mut groups: Oa3Group<'_> = Vec::new();

    let Some(paths) = value.get("paths").and_then(|p| p.as_object()) else {
        return groups;
    };

    let http_methods = ["get", "post", "put", "patch", "delete", "head", "options", "trace"];

    for (path, path_val) in paths {
        for method in http_methods {
            if let Some(op) = path_val.get(method) {
                let tags: Vec<&str> = op
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
                    .unwrap_or_else(|| vec!["default"]);
                let tags = if tags.is_empty() { vec!["default"] } else { tags };

                for tag in tags {
                    match groups.iter_mut().find(|(t, _)| t == tag) {
                        Some(g) => g.1.push((path.as_str(), method, op)),
                        None => groups.push((tag.to_string(), vec![(path.as_str(), method, op)])),
                    }
                }
            }
        }
    }

    groups
}

fn render_oa3_op(path: &str, method: &str, op: &Value) -> String {
    let anchor = op_anchor(method, path);
    let summary = sv(op, "summary").unwrap_or_default();
    let deprecated = op.get("deprecated").and_then(|d| d.as_bool()).unwrap_or(false);
    let dep_cls = if deprecated { " deprecated" } else { "" };
    let dep_badge = if deprecated { r#"<span class="dep-badge">DEPRECATED</span>"# } else { "" };

    let mut body = String::new();

    if let Some(desc) = sv(op, "description") {
        body.push_str(&format!("<p class=\"op-desc\">{}</p>", simple_md(&desc)));
    }
    if let Some(oid) = sv(op, "operationId") {
        body.push_str(&format!(
            "<div class=\"op-id\">operationId: <code>{}</code></div>",
            esc(&oid)
        ));
    }

    // Parameters
    if let Some(params) = op.get("parameters").and_then(|p| p.as_array()) {
        if !params.is_empty() {
            body.push_str(
                "<div class=\"params-section\"><h4 class=\"section-label\">Parameters</h4>\
                 <table class=\"params-table\">\
                 <thead><tr><th>Name</th><th>In</th><th>Type</th><th>Req</th><th>Description</th></tr></thead>\
                 <tbody>",
            );
            for p in params {
                let name = sv(p, "name").unwrap_or_default();
                let loc = sv(p, "in").unwrap_or_default();
                let req = p.get("required").and_then(|r| r.as_bool()).unwrap_or(loc == "path");
                let type_str = p
                    .pointer("/schema/type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("any");
                let desc = sv(p, "description").unwrap_or_default();
                let req_cls = if req { " row-required" } else { "" };
                body.push_str(&format!(
                    r#"<tr class="{req_cls}"><td><code>{name}</code></td><td><span class="param-in param-in-{loc_lc}">{loc}</span></td><td><code class="type-str">{type_str}</code></td><td>{req_mark}</td><td>{desc}</td></tr>"#,
                    name = esc(&name),
                    loc = esc(&loc),
                    loc_lc = loc.to_lowercase(),
                    req_mark = if req { "✓" } else { "" },
                    desc = esc(&desc),
                ));
            }
            body.push_str("</tbody></table></div>");
        }
    }

    // Request body
    if let Some(rb) = op.get("requestBody") {
        let req = rb.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
        let req_label = if req { " <span class=\"required-label\">required</span>" } else { "" };
        body.push_str(&format!(
            "<div class=\"body-section\"><h4 class=\"section-label\">Request Body{req_label}</h4>"
        ));
        if let Some(desc) = sv(rb, "description") {
            body.push_str(&format!("<p class=\"body-desc\">{}</p>", esc(&desc)));
        }
        if let Some(content) = rb.get("content").and_then(|c| c.as_object()) {
            for (mt, mt_val) in content {
                body.push_str(&format!(
                    "<span class=\"content-type\">{}</span>",
                    esc(mt)
                ));
                if let Some(schema) = mt_val.get("schema") {
                    body.push_str(&format!(
                        "<pre class=\"code-block\">{}</pre>",
                        esc(&pretty(schema))
                    ));
                }
            }
        }
        body.push_str("</div>");
    }

    // Responses
    if let Some(responses) = op.get("responses").and_then(|r| r.as_object()) {
        body.push_str("<div class=\"resp-section\"><h4 class=\"section-label\">Responses</h4>");
        let mut codes: Vec<(&String, &Value)> = responses.iter().collect();
        codes.sort_by_key(|(k, _)| k.as_str());
        for (code, resp) in codes {
            let cls = status_class(code);
            let desc = sv(resp, "description").unwrap_or_default();
            body.push_str(&format!(
                r#"<div class="resp-item"><div class="resp-header"><span class="status-code status-{cls}">{code}</span> <span class="resp-desc">{desc}</span></div>"#,
                code = esc(code),
                desc = esc(&desc),
            ));
            if let Some(content) = resp.get("content").and_then(|c| c.as_object()) {
                for (mt, mt_val) in content {
                    body.push_str(&format!("<div class=\"media-type\"><span class=\"content-type\">{}</span>", esc(mt)));
                    if let Some(schema) = mt_val.get("schema") {
                        body.push_str(&format!(
                            "<pre class=\"code-block resp-schema\">{}</pre>",
                            esc(&pretty(schema))
                        ));
                    }
                    body.push_str("</div>");
                }
            }
            body.push_str("</div>");
        }
        body.push_str("</div>");
    }

    format!(
        r##"<div class="operation{dep_cls}" id="{anchor}">
  <div class="op-header" onclick="toggleOp('{anchor}')">
    <span class="badge badge-{ml}">{method}</span>
    <code class="op-path">{path_esc}</code>
    {dep_badge}
    <span class="op-summary">{summary}</span>
    <span class="chevron">▾</span>
  </div>
  <div class="op-body" id="body-{anchor}">{body}</div>
</div>"##,
        ml = method.to_lowercase(),
        path_esc = esc(path),
        summary = esc(&summary),
    )
}

// Shared sidebar builder
fn sidebar_html<T>(
    title: &str,
    version: &str,
    groups: &[(String, Vec<(&str, &str, T)>)],
    anchor_fn: impl Fn(&str, &str, &T) -> String,
) -> String {
    let mut html = format!(
        r#"<div class="sidebar-header">
  <div class="sidebar-title">{}</div>
  <div class="sidebar-version">v{}</div>
</div>
<div class="sidebar-search">
  <input type="text" id="search-input" placeholder="Filter endpoints..." oninput="filterSidebar(this.value)">
</div>
<nav class="sidebar-nav">"#,
        esc(title),
        esc(version),
    );

    for (tag, ops) in groups {
        let tag_id = slug(tag);
        html.push_str(&format!(
            r#"<div class="sidebar-group">
  <button class="sidebar-group-toggle" onclick="toggleGroup('{tag_id}')">{} <span class="chevron">▾</span></button>
  <div class="sidebar-group-items" id="grp-{tag_id}">"#,
            esc(tag),
        ));
        for (path, method, op) in ops {
            let anchor = anchor_fn(path, method, op);
            // href="#anchor" needs r## to avoid "# terminating r#"..."#
            html.push_str(&format!(
                r##"<a class="sidebar-item" href="#{anchor}" data-method="{method}">
  <span class="badge badge-{ml}">{method}</span>
  <span class="item-label">{path}</span>
</a>"##,
                ml = method.to_lowercase(),
                path = esc(path),
            ));
        }
        html.push_str("</div></div>");
    }

    html.push_str("</nav>");
    html.push_str(&format!(
        r##"<footer class="sidebar-footer">
  <a href="https://github.com/affolter-engineering/oa-converter" class="sidebar-footer-link">oa-converter</a>
  <span class="sidebar-footer-ver">v{}</span>
</footer>"##,
        env!("CARGO_PKG_VERSION"),
    ));
    html
}

// Shared page wrapper
fn page(title: &str, sidebar: &str, content: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{}</title>
  <style>{CSS}</style>
</head>
<body>
  <div id="app">
    <nav id="sidebar">{sidebar}</nav>
    <main id="content">{content}</main>
  </div>
  <script>{JS}</script>
</body>
</html>"#,
        esc(title),
    )
}

// Utilities
fn op_anchor(method: &str, path: &str) -> String {
    let p = path
        .replace('/', "-")
        .replace(['{', '}'], "")
        .trim_matches('-')
        .to_string();
    format!("op-{}-{}", method.to_lowercase(), p)
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn sv(v: &Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(String::from)
}

fn status_class(code: &str) -> &'static str {
    match code.chars().next() {
        Some('2') => "2xx",
        Some('3') => "3xx",
        Some('4') => "4xx",
        Some('5') => "5xx",
        _ => "other",
    }
}

fn simple_md(text: &str) -> String {
    esc(text).split('\n').map(|line| {
        let mut l = line.to_string();
        loop {
            let Some(a) = l.find("**") else { break };
            let Some(b) = l[a + 2..].find("**").map(|i| i + a + 2) else { break };
            let inner = l[a + 2..b].to_string();
            l = format!("{}<strong>{inner}</strong>{}", &l[..a], &l[b + 2..]);
        }
        loop {
            let Some(a) = l.find('`') else { break };
            let Some(b) = l[a + 1..].find('`').map(|i| i + a + 1) else { break };
            let inner = l[a + 1..b].to_string();
            l = format!("{}<code>{inner}</code>{}", &l[..a], &l[b + 1..]);
        }
        l
    }).collect::<Vec<_>>().join("<br>")
}

// Embedded CSS
const CSS: &str = r#"
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
:root{
  --sidebar:280px;
  --bg-sidebar:#1a1d23;--bg:#fff;--text:#24292f;--border:#e1e4e8;--code-bg:#f6f8fa;
  --mono:'SFMono-Regular',Consolas,'Liberation Mono',Menlo,monospace;
  --sans:-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif;
  --get:#61affe;--post:#49cc90;--put:#fca130;--patch:#50e3c2;
  --delete:#f93e3e;--head:#9012fe;--options:#0d5aa7;--trace:#785446
}
body{font-family:var(--sans);color:var(--text);background:var(--bg)}
#app{display:flex;min-height:100vh}
#sidebar{
  width:var(--sidebar);position:fixed;top:0;left:0;bottom:0;
  background:var(--bg-sidebar);color:#c9d1d9;overflow-y:auto;
  display:flex;flex-direction:column;z-index:100
}
.sidebar-header{padding:20px 16px 12px;border-bottom:1px solid #30363d}
.sidebar-title{font-size:14px;font-weight:600;color:#e6edf3;word-break:break-word;line-height:1.4}
.sidebar-version{font-size:11px;color:#8b949e;margin-top:4px}
.sidebar-search{padding:10px 12px 6px}
.sidebar-search input{width:100%;padding:6px 10px;background:#2d3139;border:1px solid #30363d;border-radius:6px;color:#c9d1d9;font-size:13px;outline:none}
.sidebar-search input:focus{border-color:#58a6ff}
.sidebar-nav{flex:1;overflow-y:auto;padding-bottom:20px}
.sidebar-group{margin-bottom:2px}
.sidebar-group-toggle{display:flex;justify-content:space-between;align-items:center;width:100%;padding:8px 16px;background:none;border:none;color:#8b949e;font-size:11px;font-weight:700;text-transform:uppercase;letter-spacing:.08em;cursor:pointer}
.sidebar-group-toggle:hover{color:#e6edf3}
.sidebar-item{display:flex;align-items:center;gap:8px;padding:6px 16px 6px 24px;text-decoration:none;color:#c9d1d9;font-size:13px;border-left:2px solid transparent}
.sidebar-item:hover{background:#2d3139}
.sidebar-item.active{border-left-color:#58a6ff}
.sidebar-item.hidden{display:none}
.item-label{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1}
.chevron{font-size:10px}
.sidebar-footer{padding:12px 16px;border-top:1px solid #30363d;display:flex;align-items:center;justify-content:space-between;flex-shrink:0}
.sidebar-footer-link{color:#8b949e;font-size:11px;font-weight:600;text-decoration:none}
.sidebar-footer-link:hover{color:#e6edf3}
.sidebar-footer-ver{color:#57606a;font-size:11px;font-family:var(--mono)}
#content{margin-left:var(--sidebar);flex:1;max-width:940px;padding:40px 40px 80px}
.api-header{margin-bottom:36px;padding-bottom:20px;border-bottom:2px solid var(--border)}
.api-header h1{font-size:28px;font-weight:700;margin-bottom:8px}
.api-meta{display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin-bottom:10px}
.ver-badge{background:#ddf4ff;color:#0550ae;padding:2px 8px;border-radius:20px;font-size:12px;font-weight:600}
.oa-badge{background:#f6f8fa;color:#57606a;padding:2px 8px;border-radius:20px;font-size:12px;border:1px solid var(--border)}
.api-desc{color:#57606a;line-height:1.6;margin-top:8px}
.api-links{display:flex;gap:14px;margin-top:10px}
.api-link{color:#0550ae;text-decoration:none;font-size:13px}
.api-link:hover{text-decoration:underline}
.info-section{margin-bottom:36px}
.info-section h2,.tag-section h2.tag-heading{font-size:20px;font-weight:700;margin-bottom:12px;padding-bottom:8px;border-bottom:1px solid var(--border)}
.tag-section{margin-bottom:48px}
.tag-desc{color:#57606a;margin-bottom:16px}
.server-list{list-style:none;display:flex;flex-direction:column;gap:6px}
.server-list code{font-family:var(--mono);font-size:13px}
.server-desc{color:#57606a;font-size:13px}
.scheme-card{border:1px solid var(--border);border-radius:8px;padding:12px;margin-bottom:10px}
.scheme-name{font-weight:600;font-size:14px;margin-right:8px}
.scheme-type{display:inline-block;background:#fff8c5;color:#7d4e00;padding:1px 6px;border-radius:4px;font-size:11px;font-weight:700;text-transform:uppercase}
.badge{display:inline-flex;align-items:center;justify-content:center;min-width:60px;padding:3px 8px;border-radius:4px;font-size:11px;font-weight:700;font-family:var(--mono);letter-spacing:.05em;color:#fff;flex-shrink:0}
.badge-get{background:var(--get)}.badge-post{background:var(--post)}.badge-put{background:var(--put)}
.badge-patch{background:var(--patch);color:#333}.badge-delete{background:var(--delete)}
.badge-head{background:var(--head)}.badge-options{background:var(--options)}.badge-trace{background:var(--trace)}
.dep-badge{background:#ffdcd7;color:#82071e;font-size:10px;font-weight:700;padding:2px 6px;border-radius:4px;text-transform:uppercase}
.operation{border:1px solid var(--border);border-radius:8px;margin-bottom:10px;overflow:hidden}
.operation.deprecated{opacity:.7}
.op-header{display:flex;align-items:center;gap:10px;padding:12px 16px;cursor:pointer;background:#f6f8fa;flex-wrap:wrap;user-select:none}
.op-header:hover{background:#eaeef2}
.op-path{font-family:var(--mono);font-size:14px}
.op-summary{color:#57606a;font-size:13px;flex:1}
.op-body{display:none;padding:20px;border-top:1px solid var(--border)}
.op-body.open{display:block}
.op-desc{color:#57606a;font-size:14px;line-height:1.6;margin-bottom:14px}
.op-id{font-size:12px;color:#8b949e;margin-bottom:12px}
.params-section,.body-section,.resp-section{margin-bottom:20px}
.section-label{font-size:13px;font-weight:700;color:#57606a;text-transform:uppercase;letter-spacing:.05em;margin-bottom:8px;display:block}
.params-table{width:100%;border-collapse:collapse;font-size:13px}
.params-table th{padding:7px 10px;background:#f6f8fa;border:1px solid var(--border);font-weight:600;color:#57606a;font-size:11px;text-transform:uppercase}
.params-table td{padding:7px 10px;border:1px solid var(--border);vertical-align:top}
.params-table tr.row-required td:first-child{border-left:3px solid #2da44e}
.type-str{font-family:var(--mono);font-size:12px;color:#0550ae}
.param-in{display:inline-block;padding:1px 5px;border-radius:4px;font-size:11px;font-weight:600;text-transform:uppercase}
.param-in-path{background:#fff3cd;color:#7d4e00}.param-in-query{background:#e7f3fe;color:#0550ae}
.param-in-header{background:#f3e7fe;color:#6e40c9}.param-in-cookie{background:#fce8e6;color:#9e2a2b}
.param-in-body{background:#e8f5e9;color:#116329}
.required-label{background:#ffdcd7;color:#82071e;font-size:11px;padding:1px 5px;border-radius:4px;margin-left:6px}
.body-desc{color:#57606a;font-size:13px;margin-bottom:8px}
.content-type{display:inline-block;background:var(--code-bg);border:1px solid var(--border);border-radius:4px;padding:2px 7px;font-family:var(--mono);font-size:12px;color:#57606a;margin-bottom:6px}
.media-type{margin-top:8px}
.resp-item{margin-bottom:10px;border:1px solid var(--border);border-radius:6px;overflow:hidden}
.resp-header{display:flex;align-items:center;gap:8px;padding:7px 12px;background:#f6f8fa}
.status-code{font-family:var(--mono);font-size:13px;font-weight:700;padding:2px 7px;border-radius:4px}
.status-2xx{background:#d3f9d8;color:#116329}.status-3xx{background:#f6f8fa;color:#57606a;border:1px solid var(--border)}
.status-4xx{background:#ffdcd7;color:#82071e}.status-5xx{background:#f9d0cc;color:#9e2a2b}.status-other{background:#f6f8fa;color:#57606a}
.resp-desc{font-size:13px;color:#57606a}
.resp-item .code-block{border-radius:0;margin:0}
.code-block{background:#0d1117;color:#e6edf3;padding:14px;border-radius:6px;font-family:var(--mono);font-size:12px;overflow-x:auto;max-height:400px;white-space:pre;display:block;margin-top:6px}
.resp-schema{max-height:300px}
.schema-card{border:1px solid var(--border);border-radius:8px;margin-bottom:16px;overflow:hidden}
.schema-card-title{padding:10px 16px;font-size:15px;font-weight:700;background:var(--code-bg);border-bottom:1px solid var(--border)}
.schema-card .code-block{border-radius:0;max-height:500px;margin-top:0}
::-webkit-scrollbar{width:6px;height:6px}
::-webkit-scrollbar-thumb{background:#30363d;border-radius:3px}
@media(max-width:768px){#sidebar{width:100%;position:relative;height:auto}#app{flex-direction:column}#content{margin-left:0;padding:20px}}
"#;

// Embedded JS
const JS: &str = r#"
function toggleOp(id){var b=document.getElementById('body-'+id);if(b)b.classList.toggle('open')}
function toggleGroup(id){var el=document.getElementById('grp-'+id);if(el)el.style.display=el.style.display==='none'?'':'none'}
function filterSidebar(q){
  q=q.toLowerCase().trim();
  document.querySelectorAll('.sidebar-item').forEach(function(a){
    a.classList.toggle('hidden',q!==''&&a.textContent.toLowerCase().indexOf(q)===-1);
  });
}
var obs=new IntersectionObserver(function(entries){
  entries.forEach(function(e){
    if(e.isIntersecting)document.querySelectorAll('.sidebar-item').forEach(function(a){
      a.classList.toggle('active',a.getAttribute('href')==='#'+e.target.id);
    });
  });
},{threshold:0.1});
document.querySelectorAll('.operation').forEach(function(el){obs.observe(el)});
function openHash(){
  var id=window.location.hash.slice(1);
  if(!id)return;
  var b=document.getElementById('body-'+id);if(b)b.classList.add('open');
  var el=document.getElementById(id);if(el)setTimeout(function(){el.scrollIntoView({behavior:'smooth'})},50);
}
window.addEventListener('hashchange',openHash);
openHash();
"#;
