const float EXPOSURE = 0.5;

vec3 less_than(vec3 f, float value) {
    return vec3((f.x < value) ? 1.0f : 0.0f, (f.y < value) ? 1.0f : 0.0f,
                (f.z < value) ? 1.0f : 0.0f);
}

vec3 linear_to_srgb(vec3 rgb) {
    rgb = clamp(rgb, 0.0f, 1.0f);

    return mix(pow(rgb, vec3(1.0f / 2.4f)) * 1.055f - 0.055f, rgb * 12.92f,
               less_than(rgb, 0.0031308f));
}

vec3 srgb_to_linear(vec3 rgb) {
    rgb = clamp(rgb, 0.0f, 1.0f);

    return mix(pow(((rgb + 0.055f) / 1.055f), vec3(2.4f)), rgb / 12.92f,
               less_than(rgb, 0.04045f));
}

// ACES tone mapping curve fit to go from HDR to LDR
//https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
vec3 ACESFilm(vec3 x) {
    float a = 2.51f;
    float b = 0.03f;
    float c = 2.43f;
    float d = 0.59f;
    float e = 0.14f;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), 0.0f, 1.0f);
}

float fresnel_refelect_amount(float n1,
                             float n2,
                             vec3 normal,
                             vec3 incident,
                             float f0,
                             float f90) {
    float r0 = (n1 - n2) / (n1 + n2);
    r0 *= r0;
    float cosx = -dot(normal, incident);
    if (n1 > n2) {
        float n = n1 / n2;
        float sin_t2 = n * n * (1.0 - cosx * cosx);
        if (sin_t2 > 1.0) {
            return f90;
        }
        cosx = sqrt(1.0 - sin_t2);
    }
    float x = 1.0 - cosx;
    float ret = r0 + (1.0 - r0) * x * x * x * x * x;
    return mix(f0, f90, ret);
}
