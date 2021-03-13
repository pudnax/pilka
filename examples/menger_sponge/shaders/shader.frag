#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

#include <prelude.glsl>

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;
#define T(t) (texture(t, vec2(uv.x, -uv.y)))
#define T_off(t,off) (texture(t, vec2(uv.x + off.x, -(uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
	bool mouse_pressed;
} pc;

const vec3 missColor = vec3(0.0002);
const float sphereScale = 3.0;

float worldSDF(in vec3 pos) {
    float scale = sphereScale;
    float mengerSponge = mengerSpongeSDF(pos / scale, 9) * scale;

    float dist = mengerSponge;

    return dist;
}

void main() {
	vec2 m = (pc.mouse - 0.5) * 2.0 * PI;
	m = pc.mouse;
    m.y *= -1;

	m = vec2(0.43, 0.63);
	m = (m- 0.5) * 2.0 * PI;
    m.y *= -1;

	vec3 ray_pos = vec3(3.5, 4.5, 9.0);
	ray_pos.z += m.y * 10;
    ray_pos.y += m.x * 10;

    vec3 ray_dir = vec3(uv, -1.0);

    ray_dir.xz *= rotate(radians(-25.0));
    ray_dir.yz *= rotate(radians(-25.0));
    ray_dir.yx *= rotate(radians(-6.0));

    ray_dir = normalize(ray_dir);

    vec2 dist = ray_march(ray_pos, ray_dir);

    if(dist.x > 0.0) { // hit
        vec3 col = vec3(1.0-(dist.y/float(MAX_STEPS)));
        // col = mix(1. - col, col, smoothstep(0.5, 2.0, 0.4 + sprm*4));
        out_color = vec4(col, 1.0);
    } else { // miss
        out_color = vec4(missColor, 1.0);
    }

	out_color = vec4(vec3(pow(out_color.rgb, vec3(1/0.36))), 1.0);
}
