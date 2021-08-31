float time;

/* #define COMPUTE_ROUTINE */

#define MAX_DISTANCE 10000.
#define TWO_PI 6.28318530718
#define PI 3.14159265359

/* #define time pc.time */
/* #define time 157.07131 */

#define MAX_LEVEL 5
#define bayer2x2(a) (4-(a).x-((a).y<<1))%4
float get_bayer_from_coord(vec2 pixelpos) {
    ivec2 ppos = ivec2(pixelpos);
    int sum = 0;
    for (int i = 0; i < MAX_LEVEL; i++) {
        sum += bayer2x2(ppos >> (MAX_LEVEL - 1 - i) & 1) << (2 * i);
    }

    return float(sum) / float(2 << (MAX_LEVEL * 2 - 1));
}

uvec4 s0, s1;
ivec2 pixel;

void rng_initialize(vec2 p, uint frame) {
    pixel = ivec2(p);

    //white noise seed
    s0 = uvec4(p, uint(frame), uint(p.x) + uint(p.y));

    //blue noise seed
    s1 = uvec4(frame, frame * 15843, frame * 31 + 4566, frame * 2345 + 58585);
}

// https://www.pcg-random.org/
uvec4 pcg4d(inout uvec4 v) {
    v = v * 1664525u + 1013904223u;
    v.x += v.y*v.w; v.y += v.z*v.x; v.z += v.x*v.y; v.w += v.y*v.z;
    v = v ^ (v >> 16u);
    v.x += v.y*v.w; v.y += v.z*v.x; v.z += v.x*v.y; v.w += v.y*v.z;
    return v;
}
float rand() { return float(pcg4d(s0).x) / float(0xffffffffu); }
vec2 rand2() { return vec2(pcg4d(s0).xy) / float(0xffffffffu); }
vec3 rand3() { return vec3(pcg4d(s0).xyz) / float(0xffffffffu); }
vec4 rand4() { return vec4(pcg4d(s0)) / float(0xffffffffu); }

vec3 nrand3(float sigma, vec3 mean) {
    vec4 Z = rand4();
    return mean +
           sigma * sqrt(-2.0 * log(Z.xxy)) *
               vec3(cos(TWO_PI * Z.z), sin(TWO_PI * Z.z), cos(TWO_PI * Z.w));
}

mat2 rot(float a) {
    float c = cos(a), s = sin(a);
    return mat2(c, -s, s, c);
}

float sd_sphere(vec3 p, float r) {
    return length(p) - r;
}

float sd_box(vec3 p, vec3 s) {
    p = abs(p) - s;
    return length(max(p, 0.0)) + min(max(p.x, max(p.y, p.z)), 0.0);
}

float sd_caps(vec3 p, vec3 p1, vec3 p2, float s) {
    vec3 pa = p - p1;
    vec3 pb = p2 - p1;
    float proj = dot(pa, pb) / dot(pb, pb);
    proj = clamp(proj, 0., 1.);
    return length(p1 + pb * proj - p) - s;
}

float scene2(vec3 p) {
    vec3 p2 = p;
    float t = time * 0.1;
    /* p2.yz *= rot(t); */
    /* p2.yx *= rot(t * 1.3); */
    float d = sd_box(p2, vec3(3.));
    /* d = max(d, -sd_sphere(p, 1.2)); */

    float width = 3.;
    d = max(d, -sd_caps(p2, vec3(0., -width, 0.), vec3(0., width, 0.), 1.));

    return d;
}

float op_cut_space(inout vec3 p, in vec3 n, in float w, in float sp) {
    float dt = dot(p, n) + w;
    float dcut = abs(dt) - sp;
    p -= sp * n * sign(dt);
    return dcut;
}

vec3 erot(vec3 p, vec3 ax, float ro) {
    return mix(dot(ax, p) * ax, p, cos(ro)) + cross(ax, p) * sin(ro);
}

float scene(vec3 p) {
    vec3 pos = p;
    float t = time * 0.05;
    pos.yz *= rot(t);
    pos.yx *= rot(t * 1.3);

    float d = length(p) - 1.5;

    /* pos = erot(pos, vec3(1., 0., 0.), pos.x > 0 ? time : -time); */
    pos = erot(pos, vec3(1., 0., 0.), pos.x > 0 ? 0.5 : - 0.3);
    d = sd_box(pos, vec3(2.6));

    float cut = op_cut_space(pos, vec3(1., 0., 0.), 0.0, 0.05);

    d = max(d, -cut);

    float box = sd_box(p, vec3(4.3));
    d = max(d, box);

    return d;
}

vec3 norm(vec3 p) {
    mat3 k = mat3(p, p, p) - mat3(0.001);
    return normalize(scene(p) - vec3(scene(k[0]), scene(k[1]), scene(k[2])));
}

float rnd(vec2 uv) {
    return fract(dot(sin(uv * 4532.714 + uv.yx *543.524), vec2(352.887)));
}


const int NUM_LASERS = 2;
const int LASER_LENGTH = 10;
vec3 LASERS[NUM_LASERS][LASER_LENGTH];
int NPATH[NUM_LASERS] = int[](1, 1);

float ATT = 0.;

float scene_with_lasers(vec3 p) {
    float d = scene(p);

    float d2 = 10000.;
    for (int i = 0; i < NUM_LASERS; ++i) {
        for (int laser_segment = 0; laser_segment < NPATH[i] - 1; ++laser_segment) {
            float d3 = sd_caps(p, LASERS[i][laser_segment], LASERS[i][laser_segment + 1], 0.01);
            ATT += 0.013 / (0.05 + abs(d3)) * smoothstep(4., 0.3, d3);

            d2 = min(d2, d3);
        }
    }
    return min(abs(d), d2);
}

vec3 process_hit(inout float dist, vec3 r, vec3 p, inout float side, float ior) {
    const float abberation = 0.3;
    vec2 off = vec2(0.01, 0);
    vec3 n = side * norm(p);
    n = normalize(nrand3(0.005, n));
    vec3 rn;
    if (side == 0.) {
        rn = refract(r, n,  side * (abberation + 0.1 * ior));
    } else {
        rn = refract(r, n, 1 / side * (abberation + 0.1 * ior));
    }
    rn = refract(r, n, 1 - side * (abberation + 0.1 * ior));
    /* rn = refract(r, n, 1 - side * (-2.0 + 0.1 * ior)); */
    /* rn = refract(r, n, pow(ior, -side)); */
    if (length(rn) > 0.5) {
        side *= -1;
    } else {
        rn = reflect(r, n);
    }
    dist = 0.1;
    return rn;
}

void trace_lazer(vec3 ro, vec3 rd, float ior, int n) {
    vec3 p = ro;
    LASERS[n][0] = p;
    NPATH[n] = 1;
    float side = 1.;
    for (int i = 0; i < 60; ++i) {
        float d = abs(scene(p));
        if (d < 0.001) {
            LASERS[n][NPATH[n]] = p;
            NPATH[n] += 1;
            if (NPATH[n] >= LASER_LENGTH - 1)
                break;

            rd = process_hit(d, rd, p, side, ior);
        }
        if (d > 100.0) break;
        p += rd * d;
    }
    LASERS[n][NPATH[n]] = p + rd * 1000.;
    NPATH[n] += 1;
}

void trace_lasers(vec3 ro, vec3 rd, float ior) {
    trace_lazer(ro, rd, ior, 0);

    ro *= -1.;
    rd.x *= -1.;
    trace_lazer(ro, rd, ior, 1);
}

vec3 render(vec2 frag_coords, vec2 resolution, uint frame) {
    vec2 uv = (frag_coords / resolution + -0.5) * 2.0 /
              vec2(resolution.y / resolution.x, 1);
    uv += (rand() - 0.5) * 2.0 / resolution;

    rng_initialize(frag_coords, frame);
    float bayer_jitter = get_bayer_from_coord(frag_coords);

    vec3 lazer_start = vec3(18., 0., 0.);
    vec3 lazer_dir = normalize(vec3(-1., sin(3.4) * 0.07, 0));
    /* lazer_dir = normalize(vec3(-1., sin(time) * 0.07, 0)); */

    float ior = rand() * 2.0 - 1.0;
    ior = bayer_jitter * 2.0 - 1.0;
    vec3 diff = 1.3 - vec3(1. + ior, 0.45 + abs(ior), 1. - ior);

    trace_lasers(lazer_start, lazer_dir, ior);

    vec3 s = vec3(0., 0., -10.);
    vec3 r = normalize(vec3(uv, 1.));

    float rg = rand();
    rg = mix(rg, bayer_jitter, 0.5);
    float mumu = mix(rg, 1., 0.95);
    vec3 p = s;
    float side2 = 1.;
    for (int i = 0; i < 90; ++i) {
        float d = abs(scene_with_lasers(p));
        if (d < 0.001) {
            r = process_hit(d, r, p, side2, ior);
        }
        if (d > 100.) break;
        p += r * d * mumu;
        /* p += nrand3(0.005, r) * d; */
    }

    vec3 lazer = diff * ATT;
    vec3 n = norm(p);
    vec3 color = vec3(0.);
    vec3 light_dir = normalize(vec3(1.));
    float shade = dot(light_dir, n);
    float amb = 0.2 * (mix(max(shade, 0.), shade * 0.5 + 0.5, .05));
    color = mix(vec3(amb), lazer * 1., vec3(0.9));
    /* color = lazer * 1.; */

    return color;
}
