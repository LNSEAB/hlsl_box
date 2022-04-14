#include "hlsl_box.hlsl"

struct VSInput {
    float3 position: POSITION;
};

Input vs_main(VSInput input) {
    Input output;
    output.position = float4(input.position, 1.0);
    output.coord = float2((input.position.x + 1.0) / 2.0, (1.0 - input.position.y) / 2.0);
    return output;
}
