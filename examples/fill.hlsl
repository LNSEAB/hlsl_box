#include "hlsl_box.hlsl"

float4 main(Input input): SV_TARGET {
    return float4(input.coord.x, input.coord.y, 0.0, 1.0);
}