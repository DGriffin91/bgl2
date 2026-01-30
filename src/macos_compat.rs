use bevy::platform::collections::HashMap;
use fancy_regex::{Captures, Regex};

pub fn translate_shader_to_330(vertex: &mut String, fragment: &mut String) {
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut next_location: usize = 0;

    extract_attributes(&vertex, &mut map, &mut next_location);
    extract_attributes(&fragment, &mut map, &mut next_location);

    *vertex = rewrite_attributes(vertex, &map);
    *fragment = rewrite_attributes(fragment, &map);

    *vertex = vertex.replace("varying ", "out ");
    *fragment = fragment
        .replace("varying ", "in ")
        .replace("gl_FragColor", "_FragColor")
        .replace("void main(", "out vec4 _FragColor;\nvoid main(");

    for shader in [vertex, fragment] {
        *shader = shader
            .replace("texture2D(", "texture(")
            .replace("textureCubeLod(", "textureLod(");
    }
}

fn extract_attributes(shader: &str, map: &mut HashMap<String, usize>, next_location: &mut usize) {
    // Match:
    //   attribute [lowp|mediump|highp]? <type> <name>[...optional array...]
    // while ignoring //-commented lines by anchoring to start and excluding lines starting with optional whitespace + //
    let re = Regex::new(
        r#"(?m)^(?!\s*//)\s*attribute\s+(?:(?:lowp|mediump|highp)\s+)?(\w+)\s+(\w+)(?:\s*\[.*?\])?\s*;"#,
    ).unwrap();

    for cap in re.captures_iter(shader) {
        let Ok(cap) = cap else {
            continue;
        };
        let name = cap[2].to_string();
        if !map.contains_key(&name) {
            map.insert(name, *next_location);
            *next_location += 1;
        }
    }
}

fn rewrite_attributes(src: &str, map: &HashMap<String, usize>) -> String {
    let re = Regex::new(
        r#"(?m)^(?!\s*//)\s*attribute\s+(?:(?:lowp|mediump|highp)\s+)?(\w+)\s+(\w+)(\s*\[.*?\])?\s*;"#,
    ).unwrap();

    re.replace_all(src, |caps: &Captures| {
        let ty = &caps[1];
        let name = &caps[2];
        let array = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let loc = map.get(name).copied().unwrap_or(0);
        format!("layout (location = {}) in {} {}{};", loc, ty, name, array)
    })
    .to_string()
}
