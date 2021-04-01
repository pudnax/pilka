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

#define TRACE_STEPS 128
#define TRACE_EPSILON .001
#define REFLECT_EPSILON .1
#define TRACE_DISTANCE 30.
#define NORMAL_EPSILON .01
#define REFLECT_DEPTH 4
#define NUM_BALLS 7
#define CUBEMAP_SIZE 128

vec3 balls[NUM_BALLS];

float touching_balls(in vec3 at) {
    float sum = 0.;
    for (int i = 0; i < NUM_BALLS; ++i) {
        float r = length(balls[i] - at);
        sum += 1. / (r * r);
    }
    return 1. - sum;
}

void update_balls(float t) {
    for (int i = 0; i < NUM_BALLS; ++i) {
        balls[i] =
            3. * vec3(sin(.3 + float(i + 1) * t), cos(1.7 + float(i - 5) * t),
                      1.1 * sin(2.3 + float(i + 7) * t));
    }
}

float world(in vec3 at) {
    return touching_balls(at);
}

vec3 normal(in vec3 at) {
    vec2 e = vec2(0., NORMAL_EPSILON);
    return normalize(vec3(world(at + e.yxx) - world(at),
                          world(at + e.xyx) - world(at),
                          world(at + e.xxy) - world(at)));
}

vec4 raymarch(in vec3 pos, in vec3 dir, in float maxL) {
	float l = 0.;
	for (int i = 0; i < TRACE_STEPS; ++i) {
		float d = world(pos + dir * l);
		if (d < TRACE_EPSILON*l) break;
		l += d;
		if (l > maxL) break;
	}
	return vec4(pos + dir * l, l);
}

vec3 lookAtDir(in vec3 dir, in vec3 pos, in vec3 at) {
    vec3 f = normalize(at - pos);
    vec3 r = cross(f, vec3(0., 1., 0.));
    vec3 u = cross(r, f);
    return normalize(dir.x * r + dir.y * u + dir.z * f);
}

vec3 cube(in vec3 v) {
    float M = max(max(abs(v.x), abs(v.y)), abs(v.z));
    float scale = (float(CUBEMAP_SIZE) - 1.) / float(CUBEMAP_SIZE);
    if (abs(v.x) != M) v.x *= scale;
    if (abs(v.y) != M) v.y *= scale;
    if (abs(v.z) != M) v.z *= scale;
    return texture(generic_texture, v.xy).xyz;
}

void main() {
    vec2 uv = (in_uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);
    float t = pc.time * .11;
    update_balls(t);

    vec3 pos = vec3(cos(2. + 4. * cos(t)) * 10., 2. + 8. * cos(t * .8),
                    10. * sin(2. + 3. * cos(t)));
    vec3 dir = lookAtDir(normalize(vec3(uv, 2.)), pos.xyz, vec3(balls[0]));

    vec3 color = vec3(0.);
    float k = 1.;
    for (int reflections = 0; reflections < REFLECT_DEPTH; ++reflections) {
        vec4 tpos = raymarch(pos, dir, TRACE_DISTANCE);
        if (tpos.w >= TRACE_DISTANCE) {
            /* color += sin(dir) + cos(dir); */
			/* color += T(generic_texture).rgb * 0.1; */
			/* color += T(generic_texture).rgb * 0.1; */

            color += cube(dir);
            break;
        }
        color += vec3(0.1) * k;
        k *= 0.6;
        dir = normalize(reflect(dir, normal(tpos.xyz)));
        pos = tpos.xyz + dir * REFLECT_EPSILON;
    }

    out_color = vec4(color, 1.0);
	/* out_color = vec4(T(generic_texture).rgb, 1.); */
}
