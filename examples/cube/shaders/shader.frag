#version 460

// Heavily inspired by Flopine's streams... HBHS and cheers!
// https://www.shadertoy.com/user/Flopine

#include <prelude.glsl>

layout(std430, push_constant) uniform PushConstant {
	vec3 pos;
	float time;
	vec2 resolution;
	vec2 mouse;
	float spectrum;
} pc;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 out_color;

/* #define PI 3.141592 */
#define TAU 6.2831853071
#define dt (mod(pc.time+PI*0.5,TAU))

// reference for animation curves: https://easings.net/
float easeInOutCirc(float x) {
  return x < 0.5 ? (1. - sqrt(1. - pow(2. * x, 2.))) / 2.
                 : (sqrt(1. - pow(-2. * x + 2., 2.)) + 1.) / 2.;
}

mat2 rot(float a) { return mat2(cos(a), sin(a), -sin(a), cos(a)); }

#define animation(time) (-1.+2.*easeInOutCirc(time))
float square(vec2 uv) {
  float width = 0.35;
  uv.x += animation(sin(dt) * 0.5 + 0.5);
  uv *= rot(animation(sin(dt) * 0.5 + 0.5) * PI);
  uv = abs(uv);
  return smoothstep(width, width * 1.05, max(uv.x, uv.y));
}

float sc(vec3 p, float s) {
  p = abs(p);
  p = max(p, p.yzx);
  return min(p.x, min(p.y, p.z)) - s;
}

float cube(vec3 p) {
  p.x += animation(sin(dt) * 0.5 + 0.5) * 2.8;
  if (sin(pc.time / 2 + 0 * PI) > 0) {
    p.yz *= rot(-atan(1. / sqrt(2.)));
  } else {
    p.zy *= rot(-atan(1. / sqrt(2.)));
  }
  /* p.xz *= rot(PI / 4.); */
  p.xy *= rot(animation(sin(dt) * 0.5 + 0.5) * PI);
  return max(-sc(p, 0.8), length(max(abs(p) - vec3(1.), 0.)));
  /* return length(max(abs(p) - vec3(1.), 0.)); */
}

const vec3 PINK = vec3(212., 33., 93.) / 256;

vec3 raymarch(vec2 uv) {
    vec3 ro = vec3(uv * 3., 5.), rd = normalize(vec3(0., 0., -1.)), p = ro,
	col = vec3(0., 0.05, 0.05);
	col = PINK;
    float shad;
    bool hit = false;

    for (float i = 0.; i < 32.; i++) {
      float d = cube(p);
      if (d < 0.01) {
        hit = true;
        shad = i / 32.;
        break;
      }
      p += d * rd;
    }
    if (hit)
      col = vec3(1. - shad);
    return col;
}

void main() {

	vec3 cuber = raymarch(uv);
    vec3 col = (uv.y >= sin(dt + 2 * PI) * 2 ) ? cuber : PINK + square(uv);
    /* col = raymarch(uv); */

    out_color = vec4(col * 1.2 / (2.1 - col * 0.5), 1.0);
    out_color = vec4(pow(col, vec3(3/1.0)), 1.0);
}
