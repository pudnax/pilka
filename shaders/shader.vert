#version 450

layout(location = 0) in vec4 pos;
layout(location = 1) in vec4 color;

layout(location = 0) out vec4 o_color;
void main() {
  o_color = color;
  gl_Position = pos;
}
