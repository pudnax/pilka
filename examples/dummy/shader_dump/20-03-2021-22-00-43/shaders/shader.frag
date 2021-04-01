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
} pc;

const int MAX_STEPS = 100;
const float MISS_DIST = 100;
const float HIT_DIST = 0.001;
const vec3 EPS = vec3(0., 0.01, 0.0001);
#define PI 3.141592
#define TAU 2.0*PI

vec3 getRayDir(vec2 uv, vec3 p, vec3 l, float z) {
    vec3 f = normalize(l-p),
        r = normalize(cross(vec3(0,1,0), f)),
        u = cross(f,r),
        c = f*z,
        i = c + uv.x*r + uv.y*u,
        d = normalize(i);
    return d;
}

float smin(float a, float b) {
    // float k = 0.77521;
    float k = 0.2521;

    float h = clamp( 0.5+0.5*(b-a)/k, 0.0, 1.0 );
    return mix( b, a, h ) - k*h*(1.0-h);
}

float opBlend(float d1, float d2) {
    return smin( d1, d2 );
}

vec2 opU(vec2 d1, vec2 d2) {
    return (d1.x < d2.x) ? d1 : d2;
}

float sphereSDF(vec3 p) {
	return length(abs(p)) - 1.;
}

float boxSDF(vec3 p, vec3 b, float r) {
    return length(max(abs(p) - b, 0.0)) - r;
}

vec2 worldSDF(vec3 p) {
    float scale = 1.0;
    vec2 obj;

	vec3 tex = T(dummy_texture).rgb;

	/* vec3 p2 = sin(p/2 + vec3(0., 0., pc.time)); */
	vec3 p2 = sin(p/2 + vec3(0., 0., 0));
	p2.y += 1.;
	p2.x += 1.;
    vec2 sphere = vec2(sphereSDF(p2 / scale) * scale, 1);

	vec3 scale3 = vec3(0.2, 0.2, 0.2);
	vec3 off =  -vec3(0.0 + sin(0. + 0.2*PI), -0.7, 0.7);
	vec2 box = vec2(boxSDF(p + off, scale3, 0.2), 2.);

	/* obj = opU(box, sphere); */
	/* obj.x = opBlend(sphere.x, box.x); */

	obj = sphere;
	obj.x = mix(obj.x , tex.x, 0.5);

    return obj;
}

vec3 normal2(vec3 p) {
    mat3 eps = mat3(p, p, p) - mat3(EPS.y);
    return normalize(worldSDF(p).x - vec3(worldSDF(eps[0]).x,
                                          worldSDF(eps[1]).x,
                                          worldSDF(eps[2]).x));
}

vec3 normal(in vec3 p) {
  return normalize(vec3(worldSDF(p + EPS.yxx).x - worldSDF(p - EPS.yxx).x,
                        worldSDF(p + EPS.xyx).x - worldSDF(p - EPS.xyx).x,
                        worldSDF(p + EPS.xxy).x - worldSDF(p - EPS.xxy).x));
}

vec3 ray_march(vec3 O, vec3 D) {
	float d = 0.;

	for (int i = 0; i < MAX_STEPS; ++i) {
		vec3 pos = O + D * d;
		vec2 posToScene = worldSDF(pos);
		d += posToScene.x;
		if (abs(posToScene.x) < HIT_DIST) return vec3(d, i, posToScene.y);
		if (posToScene.x > MISS_DIST) break;
	}
	return vec3(-d, MAX_STEPS, -1);
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

	/* uv *= 1. - length(uv); */

	/* uv *= mix(uv.y, 1. - length(uv), sin((pc.time * 0.2))); */
	/* uv = abs(sin(pc.time)) > 0.5 ? uv : uv * (1. - length(uv)); */
	/* uv = smoothstep(uv, uv * (1. - length(uv)), vec2(abs(sin(pc.time)))); */

    vec3 O = vec3(0.0, 0.0, 2.4);
	O += pc.pos;
	vec3 T = vec3(1.529, -1.309, 0.0);

	vec3 ray_dir = getRayDir(uv, O, T, 1);

    vec3 path = ray_march(O, ray_dir);
    vec3 pos = O + path.x * ray_dir;
	vec3 norm = uv.y > 0 ? normal(pos) : normal2(pos);
	norm = normal(pos);
	/* norm = normal2(pos); */

    vec3 col = vec3(0.02, 0.021, 0.02);
	vec3 light_pos = vec3(1.0, 4.0, 3.0);

	vec3 light_dir = normalize(light_pos - pos);

	float match = max(dot(light_dir, norm), 0.0);

	if (path.z == 1) {
		vec3 balloon_col = vec3(1.0, 0.0, 0.0);
		col = balloon_col * match + vec3(.3, .1, .2);
	} else if (path.z == 2) {
		col = norm;
		vec3 box_col = vec3(.2, 0.9, .4);
		col = box_col * match;
	}

	/* vec3 prev = T(previous_frame).rgb; */
	/* vec3 sum = (col + prev.rrr * 0.6) / 2; */

	vec3 sum = col;

    out_color = vec4(sum, 1.0);
}
