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

#define PI 3.141592
#define TAU 2.*PI
const vec3 EPS = vec3(0., 0.01, 0.0001);
const uint MAX_STEPS = 100;
const float HIT_DIST = 0.0001;
const float MISS_DIST = 10000.;

vec2 rotate(vec2 p, float a) {
	float c = cos(a), s = sin(a);
	mat2 m = mat2(c, -s, s, c);
	return p*m;
}

vec3 look_at(vec2 uv, vec3 o, vec3 t, float z) {
	vec3 f = normalize(t - o),
		 r = normalize(cross(vec3(0., 1., 0.), f)),
		 u = cross(f, r),
		 c = f*z,
		 i = c + uv.x * r + uv.y * u,
		 d = normalize(i);
	return d;
}

float torusSDF(vec3 p, vec2 t) {
	vec2 q = vec2(length(p.xz) - t.x, p.y);
	return length(q) - t.y;
}

float boxSDF(vec3 p, vec3 b) {
	return length(max(abs(p) - b, vec3(0))) - 0.5;
}

float worldSDF(vec3 p) {
	vec3 p2 = p;
    p2.xy = rotate(p.xy, PI / 2);
    float torus = torusSDF(p2, vec2(1., 0.25));

	float box = boxSDF(p, vec3(pc.pos));

	float res = 0;

	res = max(box, -torus);

	return res;
}

float ray_march(vec3 ro, vec3 rd) {
	float d = 0.;

	for (int i = 0; i < MAX_STEPS; ++i) {
		float hit_pos = worldSDF(ro + d*rd);
		d += hit_pos;
		if (abs(hit_pos) < HIT_DIST) return d;
		if (hit_pos > MISS_DIST) break;
	}
	return -d;
}

void main() {
    vec2 uv = (in_uv - 0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    vec3 O = vec3(0., 0., -5.0);
    O.xz -= vec2(0.73, -0.84) * 5.;

    vec3 D = normalize(vec3(uv, 1.));
	D = look_at(uv, O, vec3(0.), 1.);

    O.yz = rotate(O.yz, atan(1, sqrt(2)));
    D.yz = rotate(D.yz, atan(1, sqrt(2)));

    O.xz = rotate(O.xz, PI / 4);
    D.xz = rotate(D.xz, PI / 4);

    float d = 0.;
    /* for (int i = 0; i < 100; i++) { */
    /*     d += worldSDF(O + D * d); */
    /* } */
	d = ray_march(O, D);
    vec3 pos = O + D * d;
    vec3 norm =
        normalize(vec3(worldSDF(pos + EPS.yxx) - worldSDF(pos - EPS.yxx),
                       worldSDF(pos + EPS.xyx) - worldSDF(pos - EPS.xyx),
                       worldSDF(pos + EPS.xxy) - worldSDF(pos - EPS.xxy)));

    out_color = vec4(norm * 0.5 + .5, 1.0);
    out_color = vec4(fract(pos + 0.01), 1.0);
}
