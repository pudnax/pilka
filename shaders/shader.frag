#version 450

layout(location = 0) in vec2 in_uv;

layout(location = 0) out vec4 out_color;

void main() {
	vec2 uv = in_uv;
    out_color = vec4(uv + vec2(0.5), 0.0, 1.0);
}
