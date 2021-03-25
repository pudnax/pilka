#version 460

// In the beginning, colours never existed. There's nothing that can be done before you...

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
	uint frame;
} pc;

#define rot(a) mat2(cos(a),sin(a),-sin(a),cos(a))
#define PI acos(-1.)
#define TAU (atan(1.)*8.)
#define sabs(x) sqrt(x*x + 1e-2)
#define smin(a,b) (a+b-sabs(a-b))*.5
#define smax(a,b) (a+b+sabs(a-b))*.5
#define Min(a,b) (a+b-abs(a-b))*.5
#define Max(a,b) (a+b+abs(a-b))*.5

vec3 n(vec3 p) {
	return normalize(cross(dFdy(p), dFdx(p)));
}

float hexagonSDF(vec2 p, float r) {
	const vec3 k = vec3(-0.866025404, 0.5, 0.577350269);
	p = abs(p);
	p -= 2.0*min(dot(k.xy,p), 0.0) * k.xy;
	p -= vec2(clamp(p.x, -k.z*r, k.z*r), r);
	return length(p) *sign(p.y);
}

float ease(float x) {
	return exp(-x*x*35.) * 0.3;
}

float ease_abs(float x) {
	return sabs(x) + ease(x);
}

float bevel(float x) {
	return max(0.15, abs(x));
}

float bevel_max(float a, float b) {
	return (a + b + bevel(a - b)) * 0.5;
}

float ease_min(float a, float b) {
	return (a + b - ease_abs(a - b)) * 0.5;
}

float bevel_min(float a, float b) {
	return (a + b - bevel(a - b)) * 0.5;
}

float func(float x) {
	return sin(x * 2.0) * 0.3;
}

vec2 fold(vec2 p, vec2 v) {
	float g = dot(p, v);
	return (p - (g - abs(g)) * v) * vec2(sign(g), 1);
}

vec2 fold90(vec2 p) {
    vec2 v = normalize(vec2(1. - 1.));
    float g = dot(p, v);
    return p - (g - sabs(g)) * v;
}

float de0(vec3 p) {
	vec2 q = vec2(length(p.xy) - 0.8, abs(p.z) - 0.8);
	q = fold90(q);
	float de1 = q.x;
	return q.x;
}

float de1(vec3 p) {
	p.x -= clamp(p.x, 0., 1.5);
	return length(p) - 0.3;
}

vec2 sfold(vec2 p) {
	vec2 v = normalize(vec2(1, -1));
	float g = dot(p, v);
	return p - (g - sabs(g)) * v;
}

vec2 pmod(vec2 p, float n) {
	float a = mod(atan(p.y, p.x), TAU / n) - 0.5 * TAU / n;
	return length(p) * vec2(sin(a), cos(a));
}

void signedSFold(inout vec2 p, vec2 v) {
    float g = dot(p, v);
    p = (p - (g - sabs(g)) * v) * vec2(sign(g), 1);
}

void sFold90(inout vec2 p) {
    vec2 v = normalize(vec2(1, -1));

    float g = dot(p, v);
    p -= (g - sabs(g)) * v;
}

float box(vec3 p, vec3 s) {
    p = abs(p) - s;
    sFold90(p.xz);
    sFold90(p.yz);
    sFold90(p.xy);
    return p.x;
}

float scale = 0;
float map(vec3 p) {
    float time = pc.time;
    /* p.xy *= rot(time * .5); */
    /* p.xz *= rot(time * .3); */

	// Tube
    /* float h = sin(time) * .5 + .5; */
    /* p.x -= clamp(p.x, -h, h); */
    /* return length(vec2(length(p.xy) - .3, p.z)) - .1; */

	// Box
    /* float r = .1; */
    /* vec3 v = vec3(1, 2, 1); */
    /* p -= clamp(p, -v, v); */
    /* return length(p) - r; */

	// Hexagon
	/* float sdf2d = hexagonSDF(p.xy, 0.8); */
	/* return hexagonSDF(vec2(sdf2d, p.z), 0.3 + abs(clamp(pc.spectrum, 0.1, 1))); */

	// Cylinder
	/* float sdf2d = abs(length(p.xy) - 1.) - 0.2; */
	/* float d = abs(p.z) - 0.3; */
	/* return max(sdf2d, d); */

	// Func
    /* p.y -= func(p.x); */
    /* float e = 1e-2; */
    /* float g = (func(p.x + e) - func(p.x - e)) / (2. * e); */
    /* p.y *= 1. / sqrt(1. + g * g); */
    /* p.x -= clamp(p.x, -1., 1.); */
    /* float sdf2d = length(p.xy) - 0.5; */
    /* return bevel_max(sdf2d, abs(p.z) - 0.2) * 0.9; */

	// Strip
	/* p.xy = fold(p.xy, normalize(vec2(2, 1))); */
	/* p.x += .4; */
	/* p.xy = fold(p.xy, normalize(vec2(2, -1))); */
	/* p.x -= clamp(p.x, -1., 1.); */
	/* float sdf2d = length(p.xy) - .2; */
	/* return bevel_max(sdf2d, abs(p.z) - .15); */

	// Squared star
	/* p.xy = pmod(p.xy, 5.); */
	/* p.y -= 0.6 + sin(time)*0.5; */
	/* vec3 v = vec3(0.1, 0.2, 0.2); */
	/* p -= clamp(p, -v ,v); */
	/* return length(p) - 0.05; */

	// Naked cube
	/* p = abs(p) - 0.7; */
	/* if (p.x < p.y) p.xy = p.yx; */
	/* if (p.x < p.z) p.xz = p.zx; */
	/* if (p.y < p.z) p.yz = p.zy; */
	/* return length(p.xy) - 0.1; */

	// Smooth cube
	/* p = abs(p) - vec3(0.7,0.7,1.2); */
	/* p.xz = sfold(p.xz); */
	/* p.yz = sfold(p.yz); */
	/* p.xy = sfold(p.xy); */
	/* return p.x; */

	// ???
    /* p.xy = vec2(atan(p.x, p.y) / PI * 3., length(p.xy) - 1.); */
    /* p.x = mod(p.x, 1.) - .5; */
    /* p.y = abs(p.y) - .1; */
    /* p.x = abs(p.x); */
    /* p.x -= .2; */
    /* /1* signedSFold(p.xy, normalize(vec2(2, 1))); *1/ */
    /* p.x += .05; */
    /* /1* signedSFold(p.xy, normalize(vec2(2, -1))); *1/ */
    /* p.z = abs(p.z) - .5; */
    /* return box(p, vec3(.3, .05, .25)); */

	// IFS
    /* p.x -= 4.; */
    /* p.z += time * 3.; */
    /* p = mod(p, 8.) - 4.; */
    /* for (int j = 0; j < 3; j++) { */
    /*     p.xy = abs(p.xy) - .3; */
    /*     p.yz = abs(p.yz) - sin(time * 2.) * .3 + .1, p.xz = abs(p.xz) - .2; */
    /* } */
    /* return length(cross(p, vec3(.5))) - .1; */

    /* p.z -= -time * 2.; */
    /* p.z = mod(p.z, 2.) - 1.0; */
    /* for (int i = 0; i < 8; i++) { */
    /*     p.xy = pmod(p.xy, 8.); */
    /*     p.y -= 2.; */
    /* } */
    /* p.yz = pmod(p.yz, 8.); */
    /* return dot(abs(p), normalize(vec3(7, 3, 6))) - .7; */

    /* p.xy *= rot(time * .3); */
    /* p.yz *= rot(time * .2); */
    /* for (int i = 0; i < 4; i++) { */
    /*     p.xy = pmod(p.xy, 10.); */
    /*     p.y -= 2.; */
    /*     p.yz = pmod(p.yz, 12.); */
    /*     p.z -= 10.; */
    /* } */
    /* return dot(abs(p), normalize(vec3(13, 1, 7))) - .7; */

    /* p.xy *= rot(time * .5); */
    /* p.xz *= rot(time * .8); */
    /* float s = 1.; */
    /* for (int i = 0; i < 3; i++) { */
    /*     p = abs(p) - .3; */
    /*     if (p.x < p.y) */
    /*         p.xy = p.yx; */
    /*     if (p.x < p.z) */
    /*         p.xz = p.zx; */
    /*     if (p.y < p.z) */
    /*         p.yz = p.zy; */
    /*     p.xy = abs(p.xy) - .2; */
    /*     p.xy *= rot(.3); */
    /*     p.yz *= rot(.3); */
    /*     p *= 2.; */
    /*     s *= 2.; */
    /* } */
    /* p /= s; */
    /* float h = .5; */
    /* p.x -= clamp(p.x, -h, h); */
    /* // torus SDF */
    /* return length(vec2(length(p.xy) - .5, p.z)) - .05; */

    p.z -= -time * 3.;
    p.xy = abs(p.xy) - 2.;
    if (p.x < p.y)
        p.xy = p.yx;
    p.z = mod(p.z, 4.) - 2.;
    p.x -= 3.2;
    p = abs(p);
    float s = 2.;
    vec3 offset = p * 1.5;
    for (float i = 0.; i < 5.; i++) {
        p = 1. - abs(p - 1.);
        float r = -7.5 * clamp(.38 * max(1.2 / dot(p, p), 1.), 0., 1.);
        s *= r;
        p *= r;
        p += offset;
    }
    s = abs(s);
    scale = s;
    float a = 100.;
    p -= clamp(p, -a, a);
    return length(p) / s;

	// Spiral
    /* p.xy *= rot(time * .5); */
    /* p.xz *= rot(time * .3); */
    /* float c = .2; */
    /* p.z += atan(p.y, p.x) / PI * c; */
    /* p.z = mod(p.z, c * 2.) - c; */
    /* return length(vec2(length(p.xy) - .4, p.z)) - .1; */
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    vec3 rd = normalize(vec3(uv, 1));
    vec3 p = vec3(0, 0, -3);
    float d = 1., i = 0;
    for (; ++i < 99. && d > .001;)
        p += rd * (d = map(p));
    out_color = vec4(vec3(0.), 1.);
    if (scale == 0) {
        scale = 10;
    }
    /* if (d < .001) out_color += 3. / i; */
    if (d < .001)
        out_color.xyz +=
            mix(vec3(.2, .7, 4.), (cos(vec3(3, 2, 7) + log2(scale)) * .5 + .5), .5) *
            15. / i;
}
