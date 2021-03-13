#version 460

// Shamelessly stolen from https://www.shadertoy.com/view/4slSD8

#include <prelude.glsl>

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D previous_frame;
layout(set = 0, binding = 1) uniform sampler2D generic_texture;
layout(set = 0, binding = 2) uniform sampler2D dummy_texture;
layout(set = 0, binding = 3) uniform sampler2D float_texture1;
layout(set = 0, binding = 4) uniform sampler2D float_texture2;
#define T(t) (texture(t, vec2(uv.x, -uv.y)))
#define T_off(t,off) (texture(t, vec2(uv.x + off.x, -(uv.y + off.y))))

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
	bool mouse_pressed;
} pc;

#define EPS 0.01
#define STEPS 64
#define TAU 6.28318530718

float mobius(vec3 p, float b) {
	float x = p.x, y = p.y, z = p.z;
	float xx = x*x, yy = y*y, zz = z*z, y3 = yy*y, x3 = xx*x;
	float xy = xx+yy, b2 = b*2.0, zxy = z*(xx*y*3.0-y3), xyy = x*yy*3.0-x3;
    float k1 = (2.0*zxy+xyy*(xy-zz+1.0))*(b-0.1)-xy*xy*(b2+0.2);
    float k2 = b*xy*0.2+(b2-0.2)*(zxy+xyy)-xy*(b+0.1)*(xy+zz+1.0);
	return k1*k1-xy*k2*k2;
}


float torus(vec3 p) {
	vec2 t = vec2(1.0, 0.32);
	vec2 q = vec2(length(p.xy) - t.x, p.z);
	return length(q) - t.y;
}

vec3 grad(vec3 p, float b) {
	vec2 q = vec2(0.0, EPS);
	return vec3(mobius(p+q.yxx, b) - mobius(p-q.yxx, b),
			    mobius(p+q.xyx, b) - mobius(p-q.xyx, b),
			    mobius(p+q.xxy, b) - mobius(p-q.xxy, b));
}
mat3 rotY(float ang) {
	float c = cos(ang), s = sin(ang);
	return mat3(c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c);
}

mat3 rotX(float ang) {
	float c = cos(ang), s = sin(ang);
	return mat3(1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c);
}

vec3 shade(vec3 p, vec3 rd, float b, mat3 m) {
	vec3 col = vec3(0.0);
	vec3 n = normalize(-grad(p, b));

	// material
	vec3  amb = vec3(0.05375, 0.05, 0.06625);
	vec3  dif = vec3(0.18275, 0.17, 0.22525);
	vec3  spe = vec3(0.332741, 0.328634, 0.346435);
	float shin = 39.4;

	// key light
	vec3 l = normalize(m*vec3(1.0));
	vec3 h = normalize(l-rd);
	float lambert = max(0.0, dot(n, l));
	float blinn = lambert > 0.0 ? pow(max(0.0, dot(n, h)), shin) : 0.0;
	col += vec3(3.0, 2.0, 3.0)*(0.4*dif*lambert + 1.4*spe*blinn + 0.1*amb);

	// fill light
	lambert = max(0.0, dot(n, -rd));
	blinn = lambert > 0.0 ? pow(lambert, shin) : 0.0;
	col += vec3(1.0)*(0.4*dif*lambert + 1.4*spe*blinn + 0.1*amb);

	// rim light
	col += 2.25*pow(clamp(1.0+dot(n, rd), 0.0, 1.0), 3.0);

	return col/(col+1.0); // reinhard
}

void main() {
    vec2 fc = uv.xy / pc.resolution.xy;
    vec2 uu = (uv + -0.5) * 2.0 / vec2(pc.resolution.y / pc.resolution.x, 1);

    vec2 mouse = 0.5 * TAU * (-1.0 + 2.0 * pc.mouse / pc.resolution.xy);

    mouse.x += 0.3 * pc.time;
	float b = 0.01;

    mat3 m = rotY(mouse.x) * rotX(mouse.y);
    vec3 ro = m * vec3(0.0, 0.0, 1.8);
    vec3 rd = m * normalize(vec3(uu, -1.0));

    float d = 10.0, t1 = 0.0;
    vec3 p = ro, col = vec3(1.0);

    // sphere-trace to torus envelope.
    for (int i = 0; i < STEPS; ++i) {
    	if (d < EPS || t1 > 4.0) continue;
    	d = torus(p);
    	t1 += d; p = ro + t1*rd;
    }

    if (d < EPS) {
        // forward march to find root interval.
        float t2 = t1;
        d = mobius(p, b);
        for (int i = 0; i < 2 * STEPS; ++i) {
            if (d > 0.0)
                continue;
            d = mobius(p, b);
            t2 += 2.0 * EPS;
            p = ro + t2 * rd;
        }
        // bisect towards root.
        if (d > 0.0) {
            for (int i = 0; i < 12; ++i) {
                d = 0.5 * (t1 + t2);
                p = ro + d * rd;
                if (mobius(p, b) > 0.0)
                    t2 = d;
                else
                    t1 = d;
            }
            col = shade(ro + d * rd, rd, b, m);
        }
    }

    // post-processing
    col = smoothstep(0.0, 1.0, col);
    col *=
        0.5 + 0.5 * pow(25.0 * fc.x * (1.0 - fc.x) * fc.y * (1.0 - fc.y), 0.45);
    col = pow(col, vec3(1.0 / 2.2));

    out_color = vec4(col, 1.0);
}
