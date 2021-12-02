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
#define Tuv(tex) (T(tex, in_uv))
#define T_off(tex, off) (T(tex, vec2(in_uv.x + off.x, -(in_uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
    vec3 pos;
    float time;
    vec2 resolution;
    vec2 mouse;
    bool mouse_pressed;
    uint frame;
    float time_delta;
    float record_period;
} pc;

const vec3 EPS = vec3(0., 0.01, 0.0001);
const float PI = acos(-1.);
const float TAU = 2. * PI;

// https://jbaker.graphics/writings/DEC.html
float sd_dodecahedron(vec3 p, float radius) {
    const float phi = 1.61803398875;
    const vec3 n = normalize(vec3(phi, 1, 0));

    p = abs(p / radius);
    float a = dot(p, n.xyz);
    float b = dot(p, n.zxy);
    float c = dot(p, n.yzx);
    return (max(max(a, b), c) - n.x) * radius;
}
float sd_icosahedron(vec3 p, float radius){
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
float sd_icosahestar(vec3 p){
    float radius = 1.5;
    return min(sd_dodecahedron(p, radius), sd_icosahedron(p.zyx, radius));
}

const int NUM_CUTS = 5;
vec3 CUT_DIR[NUM_CUTS];
float CUT_WIDTH[NUM_CUTS];
float CUT_OFFSET[NUM_CUTS];

void setup_cuts() {
    CUT_DIR[0] = normalize(vec3(0., 1., 0.9));
    CUT_DIR[1] = normalize(vec3(1., 0., 0.));
    CUT_DIR[2] = normalize(vec3(0., 0.6, 0.));
    CUT_DIR[3] = normalize(vec3(-1., 0.5, 0.1));
    CUT_DIR[4] = normalize(vec3(-1., 1., 0.));

    CUT_WIDTH[0] = 0.1;
    CUT_WIDTH[1] = 0.2;
    CUT_WIDTH[2] = 0.2;
    CUT_WIDTH[3] = 0.2;
    CUT_WIDTH[4] = 0.6;

    CUT_OFFSET[0] = 0.1;
    CUT_OFFSET[1] = -0.2;
    CUT_OFFSET[2] = 0.03;
    CUT_OFFSET[3] = -0.3;
    CUT_OFFSET[4] = 0.;
}

struct Cut {
    vec3 off;
    float d;
    float sign;
};

Cut op_cut(vec3 p, vec3 n, float w, float width) {
    float dt = dot(p, n) - w;
    float dcut = abs(dt) - width;
    float s = sign(dt);
    return Cut(width * n * s, dcut, s);
}

Cut merge_cuts(Cut a, Cut b) {
    // Sign computing is not right as expected,
    // but I didn't stumble on this problem yet
    return Cut(a.off + b.off, min(a.d, b.d), a.sign * b.sign);
}

Cut dummy_cut() {
    return Cut(vec3(0.), 9999., 1.);
}

// https://suricrasia.online/demoscene/functions/ blackle is a qt >:3c
vec3 erot(vec3 p, vec3 ax, float ro) {
    return mix(dot(ax, p) * ax, p, cos(ro)) + cross(ax, p) * sin(ro);
}

// https://easings.net/#easeInOutBack
float ease_in_out_back(float x){
    const float c1 = 1.70158;
    const float c2 = c1 * 1.525;

    return x < 0.5
               ? (pow(2. * x, 2.) * ((c2 + 1.) * 2. * x - c2)) / 2.
               : (pow(2. * x - 2., 2.) * ((c2 + 1.) * (x * 2. - 2.) + c2) + 2.) / 2.;
}

float map(vec3 p) {
    float time = mod(pc.time / 14., 1.);
    float dist = 999.;

    float trig_wave = min(time, 1. - time);
    float saddle = 4 * trig_wave - 0.5;

    Cut cut = dummy_cut();
    for (int i = NUM_CUTS - 1; i >= 0; --i) {
        float harm = saddle * float(NUM_CUTS) - float(i);
        float delay = 0.4;
        float release = clamp((harm - delay) / (1. - delay), 0., 1.);
        release = 0.50 - 0.5 * cos(release);
        if (release == 0.) continue;

        Cut current_cut =
            op_cut(p, CUT_DIR[i], CUT_OFFSET[i], CUT_WIDTH[i] * release);
        cut = merge_cuts(cut, current_cut);
        // Adjust position in the direction of cut
        p -= current_cut.off;

        // Rotate space on the last cut
        int last_cut = NUM_CUTS - 1;
        if (i == last_cut) {
            float rot_dir = current_cut.sign;
            p = erot(p, CUT_DIR[last_cut],
                     rot_dir * ease_in_out_back(release) * 2. * TAU);
        }
    }

    dist = sd_icosahestar(p);
    dist = max(dist, -cut.d);

    return dist;
}

mat3 get_camera(vec3 eye, vec3 at) {
    vec3 zaxis = normalize(at - eye);
    vec3 xaxis = normalize(cross(zaxis, vec3(0., 1., 0.)));
    vec3 yaxis = cross(xaxis, zaxis);
    return mat3(xaxis, yaxis, zaxis);
}

vec3 get_normal(vec3 p, float r) {
    mat3 k = mat3(p, p, p) - mat3(r);
    return normalize(vec3(map(p)) - vec3(map(k[0]), map(k[1]), map(k[2])));
}

vec3 sky(vec3 rd) {
    vec3 col = vec3(0.);
    col+=smoothstep(0.2,1.5,dot(rd, normalize(vec3(0.,-1.,0.)))) * 0.1*vec3(0.67843,0.67451,0.709);
    col+=smoothstep(.2,1.0, dot(rd, normalize(vec3(0,1,-3)))) * 0.2 * vec3(0.3647,0.2902,0.63137);
    col+=smoothstep(-0.4,0.4, dot(rd, normalize(vec3(0.9,0.2,0.6)))) * 0.2 * vec3(0.1,0.4,0.3);
    col+=smoothstep(-0.4,0.4, dot(rd + vec3(0.0,0.7,0.0), normalize(vec3(0.0,-0.2,0.0)))) *
           vec3(0.1, 0.0, 0.3) * 0.1;
    return col;
}

//https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
vec3 ACESFilm(vec3 x){
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 * vec2(pc.resolution.x / pc.resolution.y, 1);
    float time = pc.time;

    setup_cuts();

    vec3 target = vec3 (0., 0., 0.);
    float an = TAU * time / 20.;
    vec3 ro = target + 4. * vec3(cos(an), .6, sin(an));
    mat3 cam = get_camera(ro, target);
    float zoom = 1.;
    vec3 rd = cam * vec3(uv, zoom);

    const float cone_radius = .7071 / (pc.resolution.y * zoom);

    float coverage = -1.0;
    vec3 cover_dir = vec3(0.);

    const float aperture = .05;
    const float focus = 3.4;

    vec3 col = vec3(0.);

    float t = 0.;
    for (int i = 0; i < 70; ++i) {
        const float radius = t * cone_radius + aperture * abs(t - focus);
        vec3 pos = ro + t * rd;
        float dist = map(pos);

        if (dist < radius) {
            vec3 normal = get_normal(pos, radius);

            vec3 albedo = vec3(.15);
            // if the normal not looking outside of the sphere
            // so it's inner plane and should be colored
            if (dot(pos, normal) < 0.5) {
                albedo = vec3(2.5, 0.0, 0.0);
            }

            vec3 ambient =
                vec3(.1) * smoothstep(.7, 2.0, length(pos.xz) + abs(pos.y));
            vec3 directional =
                3.0 * vec3(1, .1, .13) *
                max(dot(normal, normalize(vec3(-2, -2, -1))), .0);
            directional *=
                smoothstep(.5, 1.5, dot(pos, normalize(vec3(1, 1, -1))));

            float fresnel = pow(1.0 - abs(dot(normal, rd)), 5.0);
            fresnel = mix(.03, 1.0, fresnel);

            vec3 reflection = sky(reflect(rd, normal));

            vec3 sample_color = mix(albedo * (ambient + directional),
                                    reflection, vec3(fresnel));

            // bottom light
            {
                float dif = 0.02 * clamp(0.5 - 0.5 * normal.y, 0., 1.);
                sample_color += dif;
            }

            float new_coverage = -dist / radius;
            vec3 new_coverage_dir = normalize(normal - dot(normal, rd) * rd);

            new_coverage +=
                (1.0 + coverage) * (.5 - .5 * dot(new_coverage_dir, cover_dir));
            new_coverage = min(new_coverage, 1.0);

            if (new_coverage > coverage) {
                col += sample_color * (new_coverage - coverage) * .5;

                cover_dir =
                    normalize(mix(new_coverage_dir, cover_dir,
                                  (coverage + 1.0) / (new_coverage + 1.0)));
                coverage = new_coverage;
            }
        }
        t += max(dist, radius * .5);
        if (dist < -radius || coverage > 1.0)
            break;
    }
    col += (1.0 - coverage) * .5 * sky(rd);

    // Tonemapping
    col = ACESFilm(col);

    vec2 frag_coord = in_uv * pc.resolution;
    col += sin(frag_coord.x * 314.98) * sin(frag_coord.y * 551.98) / 1024.0;

    out_color = vec4(col, 1.0);
}
