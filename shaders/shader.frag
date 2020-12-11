#version 450

layout (location = 0) out vec4 out_color;

void main() {
	vec2 uv = gl_FragCoord.xy;
    out_color = vec4(uv, 1.0, 1.0);
}
