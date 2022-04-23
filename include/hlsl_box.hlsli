struct Input {
    float4 position: SV_Position;
    float2 coord: TEXCOORD0;
};

struct Parameters {
    float2 resolution;
    float2 mouse; // left-top 0.0 ..= 1.0
    float time;
};

ConstantBuffer<Parameters> HLSLBox: register(b0);

float2 normalized_position(float2 coord) {
    return (coord * 2.0 - HLSLBox.resolution) / min(HLSLBox.resolution.x, HLSLBox.resolution.y);
}