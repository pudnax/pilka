#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    float spectrum;
    bool mouse_pressed;
    float time_delta;
} pc;

#define MAX_DIST 0.1
const vec3 EPS = vec3(0., 0.01, 0.001);
const float HIT_DIST = EPS.y;
const int MAX_STEPS = 100;
const float MISS_DIST = 10.0;

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

float scene(vec3 p, float t) {
    p.xz *= Rot(t * 5.);
    p.xy *= Rot(t * 7.);
    float scale = 0.6 + .2 * sin(t * 10.);
    p /= scale;
    return max(-sc(p, 0.8), length(max(abs(p) - vec3(1.), 0.))) * scale;
}

vec2 ray_march(vec3 rayPos, vec3 rayDir, float t) {
    float dist = 0.0;

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 pos = rayPos + (dist * rayDir);
        float posToScene = scene(pos, t);
        dist += posToScene;
        if(abs(posToScene) < HIT_DIST) return vec2(dist, i);
        if(posToScene > MISS_DIST) break;
    }

    return vec2(0, MAX_STEPS);
}

vec3 normal_vec(vec3 p, float t) {
    mat3 k = mat3(p, p, p) - mat3(EPS.z);
    return normalize(vec3(p - vec3(scene(k[0], t), scene(k[1], t), scene(k[2], t))));
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 * vec2(pc.resolution.x / pc.resolution.y, 1);

    float t = pc.time / 5;
    vec3 ro = vec3(0.0, 0.0, 4.0);
    vec3 rd = normalize(vec3(uv, -2.));
    vec3 color = vec3(0);

    for (int i = 0; i < 3; i++) {
        vec2 rm = ray_march(ro, rd, t);
        float d = rm[0];
        vec3 light = vec3(10, 0.0, 0.0);
        vec3 p = ro + rd * d;
        if (d > MAX_DIST) {
            vec3 n = normal_vec(p, t);
			vec3 dir_to_light = normalize(light - p);
			vec2 ray_march_light = ray_march(p - dir_to_light * .06, dir_to_light, t);
			float dist_to_obstacle = ray_march_light.x;
			float dist_to_light = length(light - p);
			color[i] = .5 * dot(n, dir_to_light) + 0.5;
        }
		t += .011;
    }

	/* color = vec3(uv, 1.); */
    out_color = vec4(color, 1.0);
}
