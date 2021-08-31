#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

#include <prelude.glsl>

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;
#define T(t) (texture(t, vec2(in_uv.x, -in_uv.y)))
#define T_off(t,off) (texture(t, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    float spectrum;
    bool mouse_pressed;
    float time_delta;
} pc;

#define MAX_STEPS 100
#define MAX_DIST 0.1

mat2 Rot(float a) {
    float c = cos(a), s = sin(a);
    return mat2(c, -s, s, c);
}

float sc(vec3 p, float s) {
    p = abs(p);
    p = max(p, p.yzx);
    return min(p.x, min(p.y, p.z)) - s;
}

float sdTorus(vec3 p, vec2 t) {
    vec2 q = vec2(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

float worldSDF(in vec3 p) {
    float t = pc.time / 5;
    p.xz*=Rot(t*5.);
    p.xy*=Rot(t*7.);
    float scale = 0.6 + .2*sin(t * 10.);
    p /= scale;
    return sdTorus(p, vec2(1.2, .5)) * scale;
}

vec3 getRayDir(vec2 uv, vec3 p, vec3 l, float z) {
    vec3 f = normalize(l-p),
        r = normalize(cross(vec3(0,1,0), f)),
        u = cross(f,r),
        c = f*z,
        i = c + uv.x*r + uv.y*u,
        d = normalize(i);
    return d;
}

float worldSDF2(vec3 p, float t) {
    p.xz*=Rot(t*5.);
    p.xy*=Rot(t*7.);
    float scale = 0.6 + .2*sin(t * 10.);
    p /= scale;
    return max(-sc(p, 0.8), length(max(abs(p) - vec3(1.), 0.))) * scale;
}

vec2 ray_march2(vec3 rayPos, vec3 rayDir, float t) {
    float dist = 0.0;

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 pos = rayPos + (dist * rayDir);
        float posToScene = worldSDF2(pos, t);
        dist += posToScene;
        if(abs(posToScene) < HIT_DIST) return vec2(dist, i);
        if(posToScene > MISS_DIST) break;
    }

    return vec2(0, MAX_STEPS);
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

	float t = pc.time / 5;
	vec3 O = vec3(0.0, 0.0, 4.0);
	vec3 D = normalize(vec3(uv, -2.));
	vec3 color = vec3(0);

    for (int i = 0; i < 3; i++) {
	vec2 rm = ray_march2(O, D, t);
        float d = rm[0];
        vec3 light = vec3(10,0,0);
        vec3 p = O + D * d;
        if (d > MAX_DIST) {
            vec3 n = wnormal(p);
            vec3 dirToLight = normalize(light - p);
            vec2 rayMarchLight = ray_march2(p + dirToLight * .06, dirToLight, t);
            float distToObstable = rayMarchLight.x;
            float distToLight = length(light - p);
                color[i] = .5 * (dot(n, normalize(light - p))) + .5;
                color[i] = step(
                    texture(float_texture1, (in_uv + 4.*float(i))/2.).x,
                    color[i]
                );
            } else {
		float tex = texture(float_texture1, (in_uv + 8.*float(1))/32.).x * 0.03;
		color = max(color, vec3(tex));
            }
        t += .011;
    }

    out_color = vec4(color, 1.0);
}
