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

#define PI 3.141592
const float c_minimumRayHitTime = 0.01f;
const float c_rayPosNormalNudge = 0.01f;
const float c_superFar = 10000.0f;
const float c_FOVDegrees = 90.0f;
const int c_numBounces = 8;
const int c_numRendersPerFrame = 1;
const float c_pi = 3.14159265359f;
const float c_twopi = 2.0f * c_pi;

uint wang_hash(inout uint seed) {
    seed = uint(seed ^ uint(61)) ^ uint(seed >> uint(16));
    seed *= uint(9);
    seed = seed ^ (seed >> 4);
    seed *= uint(0x27d4eb2d);
    seed = seed ^ (seed >> 15);
    return seed;
}

float RandomFloat01(inout uint state) {
    return float(wang_hash(state)) / 4294967296.0;
}

vec3 RandomUnitVector(inout uint state) {
    float z = RandomFloat01(state) * 2.0f - 1.0f;
    float a = RandomFloat01(state) * c_twopi;
    float r = sqrt(1.0f - z * z);
    float x = r * cos(a);
    float y = r * sin(a);
    return vec3(x, y, z);
}

struct SRayHitInfo {
    float dist;
	vec3 normal;
	vec3 emissive;
	vec3 albedo;
};

float ScalarTriple(vec3 u, vec3 v, vec3 w) {
    return dot(cross(u, v), w);
}

bool TestQuadTrace(in vec3 rayPos,
                   in vec3 rayDir,
                   inout SRayHitInfo info,
                   in vec3 a,
                   in vec3 b,
                   in vec3 c,
                   in vec3 d) {
    // calculate normal and flip vertices order if needed
    vec3 normal = normalize(cross(c - a, c - b));
    if (dot(normal, rayDir) > 0.0f) {
        normal *= -1.0f;

        vec3 temp = d;
        d = a;
        a = temp;

        temp = b;
        b = c;
        c = temp;
    }

    vec3 p = rayPos;
    vec3 q = rayPos + rayDir;
    vec3 pq = q - p;
    vec3 pa = a - p;
    vec3 pb = b - p;
    vec3 pc = c - p;

    // determine which triangle to test against by testing against diagonal
    // first
    vec3 m = cross(pc, pq);
    float v = dot(pa, m);
    vec3 intersectPos;
    if (v >= 0.0f) {
        // test against triangle a,b,c
        float u = -dot(pb, m);
        if (u < 0.0f)
            return false;
        float w = ScalarTriple(pq, pb, pa);
        if (w < 0.0f)
            return false;
        float denom = 1.0f / (u + v + w);
        u *= denom;
        v *= denom;
        w *= denom;
        intersectPos = u * a + v * b + w * c;
    } else {
        vec3 pd = d - p;
        float u = dot(pd, m);
        if (u < 0.0f)
            return false;
        float w = ScalarTriple(pq, pa, pd);
        if (w < 0.0f)
            return false;
        v = -v;
        float denom = 1.0f / (u + v + w);
        u *= denom;
        v *= denom;
        w *= denom;
        intersectPos = u * a + v * d + w * c;
    }

    float dist;
    if (abs(rayDir.x) > 0.1f) {
        dist = (intersectPos.x - rayPos.x) / rayDir.x;
    } else if (abs(rayDir.y) > 0.1f) {
        dist = (intersectPos.y - rayPos.y) / rayDir.y;
    } else {
        dist = (intersectPos.z - rayPos.z) / rayDir.z;
    }

    if (dist > c_minimumRayHitTime && dist < info.dist) {
        info.dist = dist;
        info.normal = normal;
        return true;
    }

    return false;
}

bool TestSphereTrace(in vec3 rayPos,
                     in vec3 rayDir,
                     inout SRayHitInfo info,
                     in vec4 sphere) {
    // get the vector from the center of this sphere to where the ray begins.
    vec3 m = rayPos - sphere.xyz;

    // get the dot product of the above vector and the ray's vector
    float b = dot(m, rayDir);

    float c = dot(m, m) - sphere.w * sphere.w;

    // exit if r's origin outside s (c > 0) and r pointing away from s (b > 0)
    if (c > 0.0 && b > 0.0)
        return false;

    // calculate discriminant
    float discr = b * b - c;

    // a negative discriminant corresponds to ray missing sphere
    if (discr < 0.0)
        return false;

    // ray now found to intersect sphere, compute smallest t value of
    // intersection
    bool fromInside = false;
    float dist = -b - sqrt(discr);
    if (dist < 0.0f) {
        fromInside = true;
        dist = -b + sqrt(discr);
    }

    if (dist > c_minimumRayHitTime && dist < info.dist) {
        info.dist = dist;
        info.normal = normalize((rayPos + rayDir * dist) - sphere.xyz) *
                      (fromInside ? -1.0f : 1.0f);
        return true;
    }

    return false;
}

void TestSceneTrace(in vec3 rayPos, in vec3 rayDir, inout SRayHitInfo hitInfo) {
    {
        vec3 A = vec3(-15.0f, -15.0f, 22.0f);
        vec3 B = vec3(15.0f, -15.0f, 22.0f);
        vec3 C = vec3(15.0f, 15.0f, 22.0f);
        vec3 D = vec3(-15.0f, 15.0f, 22.0f);
        if (TestQuadTrace(rayPos, rayDir, hitInfo, A, B, C, D)) {
            hitInfo.albedo = vec3(0.7f, 0.7f, 0.7f);
            hitInfo.emissive = vec3(0.0f, 0.0f, 0.0f);
        }
    }

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(-10.0f, 0.0f, 20.0f, 1.0f))) {
        hitInfo.albedo = vec3(1.0f, 0.1f, 0.1f);
        hitInfo.emissive = vec3(0.0f, 0.0f, 0.0f);
    }

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(0.0f, 0.0f, 20.0f, 1.0f))) {
        hitInfo.albedo = vec3(0.1f, 1.0f, 0.1f);
        hitInfo.emissive = vec3(0.0f, 0.0f, 0.0f);
    }

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(10.0f, 0.0f, 20.0f, 1.0f))) {
        hitInfo.albedo = vec3(0.1f, 0.1f, 1.0f);
        hitInfo.emissive = vec3(0.0f, 0.0f, 0.0f);
    }

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(10.0f, 10.0f, 20.0f, 5.0f))) {
        hitInfo.albedo = vec3(0.0f, 0.0f, 0.0f);
        hitInfo.emissive = vec3(1.0f, 0.9f, 0.7f) * 100.0f;
    }
}

vec3 GetColorForRay(in vec3 startRayPos,
                    in vec3 startRayDir,
                    inout uint rngState) {
    // initialize
    vec3 ret = vec3(0.0f, 0.0f, 0.0f);
    vec3 throughput = vec3(1.0f, 1.0f, 1.0f);
    vec3 rayPos = startRayPos;
    vec3 rayDir = startRayDir;

    for (int bounceIndex = 0; bounceIndex <= c_numBounces; ++bounceIndex) {
        // shoot a ray out into the world
        SRayHitInfo hitInfo;
        hitInfo.dist = c_superFar;
        TestSceneTrace(rayPos, rayDir, hitInfo);

        // if the ray missed, we are done
        /* if (hitInfo.dist == c_superFar) { */
        /*     ret += texture(iChannel1, rayDir).rgb * throughput; */
        /*     break; */
        /* } */
		if (hitInfo.dist == c_superFar)
			break;

        // update the ray position
        rayPos = (rayPos + rayDir * hitInfo.dist) +
                 hitInfo.normal * c_rayPosNormalNudge;

        // calculate new ray direction, in a cosine weighted hemisphere oriented
        // at normal
        rayDir = normalize(hitInfo.normal + RandomUnitVector(rngState));

        // add in emissive lighting
        ret += hitInfo.emissive * throughput;

        // update the colorMultiplier
        throughput *= hitInfo.albedo;
    }

    // return pixel color
    return ret;
}

vec3 GetColorForRay(in vec3 rayPos, in vec3 rayDir) {
    SRayHitInfo hitInfo;
    hitInfo.dist = c_superFar;

    vec3 ret = vec3(0.0f, 0.0f, 0.0f);

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(-10.0f, 0.0f, 20.0f, 1.0f))) {
        ret = vec3(1.0f, 0.1f, 0.1f);
    }

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(0.0f, 0.0f, 20.0f, 1.0f))) {
        ret = vec3(0.1f, 1.0f, 0.1f);
    }

    {
        vec3 A = vec3(-15.0f, -15.0f, 22.0f);
        vec3 B = vec3(15.0f, -15.0f, 22.0f);
        vec3 C = vec3(15.0f, 15.0f, 22.0f);
        vec3 D = vec3(-15.0f, 15.0f, 22.0f);
        if (TestQuadTrace(rayPos, rayDir, hitInfo, A, B, C, D)) {
            ret = vec3(0.7f, 0.7f, 0.7f);
        }
    }

    if (TestSphereTrace(rayPos, rayDir, hitInfo,
                        vec4(10.0f, 0.0f, 20.0f, 1.0f))) {
        ret = vec3(0.1f, 0.1f, 1.0f);
    }

    return ret;
}

void main() {
    uint rngState =
        uint(uint(in_uv.x) * uint(1973) + uint(in_uv.y) * uint(9277) +
             uint(pc.time) * uint(26699)) |
        uint(1);

    vec2 uv =
        ((in_uv - 0.5) * 2.0) * vec2(pc.resolution.y / pc.resolution.x, 1.0);

    vec3 ray_pos = vec3(0., 0., 0.);

    float cam_dist = 1. / tan(90 * 0.5 * PI / 180);
    vec3 ray_target = vec3(uv, cam_dist);

    vec3 ray_dir = normalize(ray_target - ray_pos);

    vec3 col = vec3(0.0);
    for (int index = 0; index < c_numRendersPerFrame; ++index)
        col += GetColorForRay(ray_pos, ray_dir, rngState) /
                 float(c_numRendersPerFrame);


    /* col = normalize(ray_target - ray_pos); */
    // average the frames together
    /* vec3 lastFrameColor = texture(iChannel0, fragCoord / iResolution.xy).rgb; */
    /* col = mix(lastFrameColor, color, 1.0f / float(iFrame+1)); */

    out_color = vec4(col, 1.0);
}

void main2() {
    // initialize a random number state based on frag coord and frame
    uint rngState =
        uint(uint(in_uv.x) * uint(1973) + uint(in_uv.y) * uint(9277) +
             uint(pc.frame) * uint(26699)) |
        uint(1);

    // The ray starts at the camera position (the origin)
    vec3 rayPosition = vec3(0.0f, 0.0f, 0.0f);

    // calculate the camera distance
    float cameraDistance = 1.0f / tan(c_FOVDegrees * 0.5f * c_pi / 180.0f);

    // calculate coordinates of the ray target on the imaginary pixel plane.
    // -1 to +1 on x,y axis. 1 unit away on the z axis
    vec3 rayTarget =
        vec3((in_uv / pc.resolution.xy) * 2.0f - 1.0f, cameraDistance);

    // correct for aspect ratio
    float aspectRatio = pc.resolution.x / pc.resolution.y;
    rayTarget.y /= aspectRatio;

    // calculate a normalized vector for the ray direction.
    // it's pointing from the ray position to the ray target.
    vec3 rayDir = normalize(rayTarget - rayPosition);

    // raytrace for this pixel
    vec3 color = vec3(0.0f, 0.0f, 0.0f);
    for (int index = 0; index < c_numRendersPerFrame; ++index)
        color += GetColorForRay(rayPosition, rayDir, rngState) /
                 float(c_numRendersPerFrame);

    /* // average the frames together */
    vec3 lastFrameColor = texture(previous_frame, in_uv / pc.resolution.xy).rgb;
    color = mix(lastFrameColor, color, 1.0f / float(pc.frame + 1));

	/* vec3 color = GetColorForRay(rayPosition, rayDir); */

    // show the result
    out_color = vec4(color, 1.0f);
}
