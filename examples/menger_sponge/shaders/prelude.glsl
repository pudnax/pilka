#define PI 3.14159265359
#define TWOPI 6.28318530718

const vec3 EPS = vec3(0., 0.01, 0.001);
const float HIT_DIST = EPS.z;
const int MAX_STEPS = 90;
const float MISS_DIST = 500.0;

const float WIDTH = 2.0;
const float HALF_WIDTH = 1.0;

float worldSDF(vec3 rayPos);

vec2 ray_march(vec3 rayPos, vec3 rayDir) {
    float dist = 0.0;

    for(int i = 0; i < MAX_STEPS; i++) {
        vec3 pos = rayPos + (dist * rayDir);
        float posToScene = worldSDF(pos);
        dist += posToScene;
        if(abs(posToScene) < HIT_DIST) return vec2(dist, i);
        if(posToScene > MISS_DIST) break;
    }

    return vec2(-dist, MAX_STEPS);
}


float crossSDF(vec3 rayPos) {
    const vec3 corner = vec3(HALF_WIDTH);
    vec3 ray = abs(rayPos);
    vec3 cornerToRay = ray - corner;
    float minComp = min(min(cornerToRay.x, cornerToRay.y), cornerToRay.z);
    float maxComp = max(max(cornerToRay.x, cornerToRay.y), cornerToRay.z);
    float midComp = cornerToRay.x + cornerToRay.y + cornerToRay.z
                                             - minComp - maxComp;
    vec2 closestOutsidePoint = max(vec2(minComp, midComp), 0.0);
    vec2 closestInsidePoint = min(vec2(midComp, maxComp), 0.0);
    return (midComp > 0.0) ? length(closestOutsidePoint) : -length(closestInsidePoint);
}

float cubeSDF(vec3 rayPos) {
    const vec3 corner = vec3(HALF_WIDTH);
    vec3 ray = abs(rayPos);
    vec3 cornerToRay = ray - corner;
    float cornerToRayMaxComponent = max(max(cornerToRay.x, cornerToRay.y), cornerToRay.z);
    float distToInsideRay = min(cornerToRayMaxComponent, 0.0);
    vec3 closestToOusideRay = max(cornerToRay, 0.0);
    return length(closestToOusideRay) + distToInsideRay;
}

float mengerSpongeSDF(vec3 rayPos, int numIterations) {
    float spongeCube = cubeSDF(rayPos);
    if (spongeCube > HIT_DIST) return spongeCube;
    float mengerSpongeDist = spongeCube;

    float scale = 1.0;
    for(int i = 0; i < numIterations; ++i) {
            float boxedWidth = WIDTH / scale;
            float translation = boxedWidth / 2.0;
            vec3 ray = rayPos + translation;
            ray = mod(ray, boxedWidth);
            ray -= boxedWidth / 2.0;
            ray *= scale;
            float crossesDist = crossSDF(ray * 3.0);
            scale *= 3.0;
            crossesDist /= scale;
            mengerSpongeDist = max(mengerSpongeDist, -crossesDist);
    }

    return mengerSpongeDist;
}

float squareSDF(vec2 rayPos) {
    const vec2 corner = vec2(HALF_WIDTH);
    vec2 ray = abs(rayPos.xy);
    vec2 cornerToRay = ray - corner;
    float cornerToRayMaxComponent = max(cornerToRay.x, cornerToRay.y);
    float distToInsideRay = min(cornerToRayMaxComponent, 0.0);
    vec2 closestToOusideRay = max(cornerToRay, 0.0);
    return length(closestToOusideRay) + distToInsideRay;
}

float sphereSDF(vec3 rayPosition, vec3 sphereCenterPosition, float radius) {
    vec3 centerToRay = rayPosition - sphereCenterPosition;
    float distToCenter = length(centerToRay);
    return distToCenter - radius;
}

float sphereSDF(vec3 rayPos, float radius) {
    return length(rayPos) - radius;
}

float sphereSDF(vec3 rayPos) {
    return length(rayPos) - HALF_WIDTH;
}

float yplaneSDF(vec3 rayPos) {
    return abs(rayPos.y);
}

mat2 rotate(float angle) {
    float sine = sin(angle);
    float cosine = cos(angle);
    return mat2(cosine, -sine, sine, cosine);
}

vec3 enlight(in vec3 at, vec3 normal, vec3 diffuse, vec3 l_color, vec3 l_pos) {
  vec3 l_dir = l_pos - at;
  return diffuse * l_color * max(0., dot(normal, normalize(l_dir))) /
         dot(l_dir, l_dir);
}

vec3 wnormal(in vec3 p) {
  return normalize(vec3(worldSDF(p + EPS.yxx) - worldSDF(p - EPS.yxx),
                        worldSDF(p + EPS.xyx) - worldSDF(p - EPS.xyx),
                        worldSDF(p + EPS.xxy) - worldSDF(p - EPS.xxy)));
}
