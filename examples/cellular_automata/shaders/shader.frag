#version 460

// In the beginning, colours never existed. There's nothing that was done before you...

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform texture2D prev_frame;
layout(set = 0, binding = 1) uniform texture2D generic_texture;
layout(set = 0, binding = 2) uniform texture2D dummy_texture;
layout(set = 0, binding = 3) uniform texture2D float_texture1;
layout(set = 0, binding = 4) uniform texture2D float_texture2;
layout(set = 1, binding = 0) uniform sampler tex_sampler;
#define T(tex, uv_coord) (texture(sampler2D(tex, tex_sampler), uv_coord))
#define Tuv(tex) (T(tex, vec2(in_uv.x, -in_uv.y)))
#define T_off(tex, off) (T(tex, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
	bool mouse_pressed;
} pc;

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    float tex = Tuv(float_texture1).r;

    float circ = distance(in_uv, vec2(0.5));
    circ = step(circ, 0.4);

    float col = min(tex, circ);

    out_color = vec4(vec3(col), 1.0);
}
