#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

#include <prelude.glsl>

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
#define T(t) (texture(t, vec2(in_uv.x, -in_uv.y)))
#define T_off(t,off) (texture(t, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;

layout(set = 1, binding = 0) uniform sampler1D fft_texture;

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	bool mouse_pressed;
    uint frame;
} pc;

#define TAU 6.28

#define BPM (120. / 60.)
#define time(speed) fract(pc.time* speed)
#define bouncy(speed) (abs(sqrt(sin(time(speed) * PI))))
#define switchanim(speed) (floor(sin(time(speed) * TAU)) + 1.)
#define animoutexpo(speed) ease_out_expo(time(speed))

#define AAstep(thre, val) \
    smoothstep(-.7, .7, (val - thre) / min(0.1, fwidth(val - thre)))
#define circle(s, puv) AAstep(s, length(puv))
#define square(s, puv) AAstep(s, max(abs(puv.x), abs(puv.y)))

#define rot(a) mat2(cos(a), sin(a), -sin(a), cos(a))

float ease_out_expo(float x) {
    return x == 1. ? 1. : 1. - pow(2., -10. * x);
}

vec3 frame(vec2 uv) {
    float timing = BPM / 2.;
    vec2 uu = uv;
    uu.y += 0.65;
    uu.y -= bouncy(timing) * 1.3;
    uu *= rot(time(timing) * PI / 2.);

    float f =
        (switchanim(timing / 2.) <= 0.) ? square(0.25, uu) : circle(0.255, uu);
    float wave = sin(abs(uv.x * 2.) - time(timing * PI) * TAU);
    float ground = mix(uv.y - wave * 0.15, uv.y, animoutexpo(timing));
    f *= AAstep(-0.65, ground);
    vec3 col = f <= 0. ? vec3(0.3, 0.1, 0.2) : vec3(0.2, 0.8, 0.5);
    return clamp(switchanim(timing / 2.) <= 0. ? col : 1. - col, 0., 1.);
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    vec3 col = frame(uv);
    out_color = vec4(sqrt(col), 1.0);
}
