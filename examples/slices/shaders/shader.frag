#version 460

// Cone tracing and effects took from TekF's shader https://www.shadertoy.com/view/MsBGWm

// In the beginning, colours never existed. There's nothing that can be done before you...

layout(location = 0) in vec2 in_uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
#define T(t) (texture(t, vec2(in_uv.x, -in_uv.y)))
#define T_off(t,off) (texture(t, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
} pc;

#define time pc.time

const float TAU = 6.28318530717958647692;

const float GAMMA = 1.1;

vec3 to_gamma(vec3 col) {
    return pow(col, vec3(1. / GAMMA));
}

void set_camera(out vec3 pos, out vec3 ray, in vec3 origin, in vec2 rotation, in float dist,
                in float zoom, in vec2 uv) {
    vec2 c = vec2(cos(rotation.x), cos(rotation.y));
    vec4 s;
    s.xy = vec2(sin(rotation.x), sin(rotation.y));
    s.zw = -s.xy;

    ray = normalize(vec3(uv, zoom));

    ray.yz = ray.yz * c.xx + ray.zy * s.zx;
    ray.xz = ray.xz * c.yy + ray.zx * s.yw;

    pos = origin - dist * vec3(c.x * s.y, s.z, c.x * c.y);
}

vec2 noise(in vec3 x) {
    vec3 p = floor(x);
    vec3 f = fract(x);
    f = f * f * (3.0 - 2.0 * f);

    vec2 uv = (p.xy + vec2(37.0, 17.0) * p.z) + f.xy;
    vec4 rg = textureLod(float_texture1, (uv + 0.5) / 256.0, 0.0);
    return mix(rg.yw, rg.xz, f.z);
}

vec3 hash3(uint n) {
    // integer hash copied from Hugo Elias
    n = (n << 13U) ^ n;
    n = n * (n * n * 15731U + 789221U) + 1376312589U;
    uvec3 k = n * uvec3(n,n*16807U,n*48271U);
    return vec3( k & uvec3(0x7fffffffU))/float(0x7fffffff);
}

float hash1(uint n) {
    // integer hash copied from Hugo Elias
    n = (n << 13U) ^ n;
    n = n * (n * n * 15731U + 789221U) + 1376312589U;
    return float( n & uvec3(0x7fffffffU))/float(0x7fffffff);
}

float op_cut_space(inout vec3 p, in vec3 n, in float w, in float sp) {
    float dt = dot(p, n) + w;
    float dcut = abs(dt) - sp;
    p -= sp * n * sign(dt);
    return dcut;
}

float tMorph;
int NB_CUTS = 4;
float opSuperCut(inout vec3 p) {
    float ksp = .04 * step(0.5, tMorph), dcut = 999.;
    uint id = uint(floor(time / 95.));
    if (ksp > 0.) {
        for (int i = 0; i < NB_CUTS; i++) {
            float w = -.4 + .8 * hash1(id + uint(i + 8989)),
                  sp = .02 + (ksp * hash1(id + uint(i + 1234)));
            vec3 n = normalize(vec3(2, 1, 1) *
                               (-1. + 2. * hash3(id * 100u + uint(i))));
            dcut = min(dcut, op_cut_space(p, n, w, sp));
        }
    }
    return dcut;
}

float sdBox(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0);
}

float sdDodecahedron(vec3 p, float radius) {
    const float phi = 1.61803398875;
    const vec3 n = normalize(vec3(phi, 1, 0));

    p = abs(p / radius);
    float a = dot(p, n.xyz);
    float b = dot(p, n.zxy);
    float c = dot(p, n.yzx);
    return (max(max(a, b), c) - n.x) * radius;
}
float sdIcosahedron(vec3 p, float radius){
    const float q = 2.61803398875;
    const vec3 n1 = normalize(vec3(q, 1, 0));
    const vec3 n2 = vec3(0.57735026919);

    p = abs(p / radius);
    float a = dot(p, n1.xyz);
    float b = dot(p, n1.zxy);
    float c = dot(p, n1.yzx);
    float d = dot(p, n2) - n1.x;
    return max(max(max(a, b), c) - n1.x, d) * radius;
}
float sdIcosahestar(vec3 p){
    float radius = 1.5;
    return min(sdDodecahedron(p, radius), sdIcosahedron(p.zyx, radius));
}

float scene(vec3 p) {
    vec3 pos = p;

    float d = sdIcosahestar(p);

    /* d = length(p) - 1.5; */
    float cut = op_cut_space(p, normalize(vec3(0., 1., 1.)), 0.1, 0.1);
    d = max(d, -cut);

    cut = op_cut_space(p, normalize(vec3(1., 1., 0.)), 0.1, 0.1);
    /* d = max(d, -cut); */

    float sc = opSuperCut(p);
    d = max(d, -sc);

    float box = sdBox(p, vec3(2.));
    d = max(d, box);

    return d;
}

vec3 sky(vec3 ray) {
    vec3 col = vec3(0.);

    col += vec3(93, 74, 161)/255. * smoothstep(.2, 1.0, dot(ray, normalize(vec3(1, 1, 3))));
    col += vec3(.1, .1, .05) * 0.01 * noise(ray * 2.0 + vec3(0, 1, 5) * time).x;
    col += 3.0 * vec3(1, 1.7, 3) * smoothstep(.8, 1.0, dot(ray, normalize(vec3(3, 3, -2))));
    col += 2.0 * vec3(2, 1, 3) * smoothstep(.9, 1.0, dot(ray, normalize(vec3(3, 8, -2))));

    return col;
}

vec3 normal_vector(vec3 p, float rep) {
    mat3 k = mat3(p, p, p) - mat3(rep);
    return normalize(scene(p) - vec3(scene(k[0]), scene(k[1]), scene(k[2])));
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    float anim = mod(time, 30.) * .1;
    /* tMorph = smoothstep(1.09,1.1,anim); */
    tMorph = 1.;

    vec3 ro = vec3(0., 1., -10);
    vec3 rd = vec3(uv, 1.);

    float zoom = 2.4;
    vec3 origin = .005 * vec3(noise(vec3(2.0 * time, 0., 0.)).xy, 0.);
    set_camera(ro, rd, origin, vec2(0.4, TAU) * (pc.mouse.yx * 0.5 + 1.) + vec2(0., time / 2.5), 6.0, zoom, uv);

    const float cone_radius = .7071 / (pc.resolution.y * zoom);

    float coverage = -1.0;
    vec3 cover_dir = vec3(0.);

    const float aperture = .05;
    const float focus = 5.0 + pc.pos.x;

    vec3 color = vec3(0.);

    float t = 0.;
    for (int i = 0; i < 100; ++i) {
        const float radius = t * cone_radius + aperture * abs(t - focus);
        vec3 p = ro + t * rd;
        const float h = scene(p);

        if (h < radius) {
            vec3 normal = normal_vector(p, radius);

            vec3 albedo = vec3(.2);

            vec3 ambient = vec3(.1) * smoothstep(.7, 2.0, length(p.xz) + abs(p.y));
            vec3 directional = 3.0 * vec3(1, .1, .13) *
                max(dot(normal, normalize(vec3(-2, -2, -1))), .0);
            directional *= smoothstep(.5, 1.5, dot(p, normalize(vec3(1, 1, -1))));

            float fresnel = pow(1.0 - abs(dot(normal, rd)), 5.0);
            fresnel = mix(.03, 1.0, fresnel);

            vec3 reflection = sky(reflect(rd, normal));

            vec3 sample_color = mix(albedo *(ambient + directional), reflection, vec3(fresnel));

            float new_coverage = -h / radius;
            vec3 new_coverage_dir = normalize(normal - dot(normal, rd) * rd);

            new_coverage += (1.0 + coverage) * (.5 - .5 * dot(new_coverage_dir, cover_dir));
            new_coverage = min(new_coverage, 1.0);

            if (new_coverage > coverage) {
                color += sample_color * (new_coverage - coverage) * .5;

                cover_dir =
                    normalize(mix(new_coverage_dir, cover_dir,
                                  (coverage + 1.0) / (new_coverage + 1.0)));
                coverage = new_coverage;
            }
        }
        t += max(h, radius * .5);
        if (h < -radius || coverage > 1.0) break;
    }
    color += (1.0 - coverage) * .5 * sky(rd);

    vec3 grainPos = vec3(in_uv  * .8, time * 30.0);
    grainPos.xy = grainPos.xy * cos(.75) + grainPos.yx * vec2(-1, 1) * sin(.75);
    grainPos.yz = grainPos.yz * cos(.5) + grainPos.zy * vec2(-1, 1) * sin(.5);
    vec2 filmNoise = noise(grainPos * .5);
    color *=
        mix(vec3(1), mix(vec3(1, .5, 0), vec3(0, .5, 1), filmNoise.x), .1 * pow(filmNoise.y, 1.0));

    uv = uv * 0.5 + 1.;
    float T = floor(time * 60.0);
    vec2 scratchSpace = mix(noise(vec3(uv * 8.0, T)).xy, uv.yx + T, .8) * 1.0;
    float scratches = texture(float_texture1, scratchSpace).r;
    color *= vec3(1.0) - .5 * vec3(.3, .5, .7) * pow(1.0 - smoothstep(.0, .1, scratches), 2.0);

    color = to_gamma(color);
    out_color = vec4(color, 1.0);
}
