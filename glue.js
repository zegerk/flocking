// JS glue: owns the rAF loop, DOM controls, and the WebGL2 renderer.
// Physics + camera + grand tour live in Rust (window.wasmBindings.Flock).
// All per-dot projection happens in the vertex shader (tour / manual 4D-5D
// chain / centroid-fit / perspective). Trails, links, marks, floor too.

import {
  calculateTrailBudget,
  maxPopulationForTrailLength,
  maxPopulationSliderValue,
  nextTrailQuality,
  populationForSliderValue,
  trailQualityLabel,
} from './trail-quality.mjs';
import { formatBuildVersion } from './version-display.mjs';
import { installPerfHarness } from './perf-harness.mjs';

// ---- palettes ----
// Four named sets. Each set supplies the four mode palettes:
//   palette (mode 0, len 5), comps (mode 1, len 8), ramp (mode 2,
//   speed=4, birth-order=8, len 7), cycle (mode 3, len 2).
// The comp-size (mode 5), cycle-length (mode 6), and in-degree (mode 7)
// modes reuse `ramp` (3 bands), a 4-stop rainbow extracted from `comps`,
// and a 4-stop subset of `palette` respectively — see palLen() below for
// the per-mode width.
const css = getComputedStyle(document.documentElement);
const cssVar = n => css.getPropertyValue('--' + n).trim();

const PALETTES = [
  { name: 'default',
    palette: ['coral','saffron','sage','peri','rose'].map(cssVar),
    comps: ['#7fb3a0','#e3c567','#9aa8d9','#e8836b','#d98ca8','#8fc7d4','#c3b184','#a99ad9'],
    ramp:  ['#6fa8b8','#7fb3a0','#a9bf83','#e3c567','#e6a15f','#e8836b','#d98ca8'],
    cycle: ['#3f4d55','#e3c567'] },
  { name: 'mono',     // user default aesthetic: pure greys
    palette: ['#3a3a3a','#5e5e5e','#828282','#b0b0b0','#d8d8d8'],
    comps:  ['#2c2c2c','#464646','#606060','#777777','#8e8e8e','#a5a5a5','#c2c2c2','#eaeaea'],
    ramp:   ['#2a2a2a','#404040','#595959','#737373','#969696','#bdbdbd','#f2f2f2'],
    cycle:  ['#2a2a2a','#ededed'] },
  { name: 'contrast', // Okabe-Ito components + viridis-short ramp
    palette: ['#E69F00','#56B4E9','#009E73','#F0E442','#0072B2'],
    comps:  ['#E69F00','#56B4E9','#009E73','#F0E442','#0072B2','#D55E00','#CC79A7','#000000'],
    ramp:   ['#440154','#414487','#2a788e','#22a884','#7ad151','#fde725','#fde725'],
    cycle:  ['#56B4E9','#E69F00'] },
  { name: 'viridis',
    palette: ['#440154','#3b528b','#21908d','#5dc863','#fde725'],
    comps:  ['#440154','#414487','#2a788e','#22a884','#7ad151','#fde725','#e6a15f','#d98ca8'],
    ramp:   ['#440154','#414487','#2a788e','#22a884','#7ad151','#fde725','#fde725'],
    cycle:  ['#440154','#fde725'] },
  { name: 'ember',    // molten: red→gold, dramatic against the dark field
    palette: ['#ff6b3d','#ffae42','#ffd166','#e8553d','#c9302c'],
    comps:  ['#3a0a0a','#7a1818','#b03020','#d04a1a','#e8741c','#ff9a3c','#ffb84d','#ffe080'],
    ramp:   ['#3a0a0a','#7a1818','#b03020','#d04a1a','#ff9a3c','#ffb84d','#ffe080'],
    cycle:  ['#1a1a1a','#ffae42'] },
  { name: 'ocean',    // aquatic: deep blue→cyan, cool & fluid
    palette: ['#1b6ca8','#3fc1c9','#7fdbda','#a8d8ea','#5a89c2'],
    comps:  ['#0a2a3a','#0e4d6e','#1b6ca8','#2a89c2','#3fc1c9','#5fd0d8','#85d8e0','#b0e6ee'],
    ramp:   ['#0a2a3a','#0e4d6e','#1b6ca8','#2a89c2','#3fc1c9','#5fd0d8','#b0e6ee'],
    cycle:  ['#0a2a3a','#3fc1c9'] },
  { name: 'aurora',   // northern lights: violet→green→pink, glowing
    palette: ['#7c4dff','#33d17a','#ff5ea0','#8be9fd','#bd93f9'],
    comps:  ['#2a1057','#4a2080','#7c4dff','#9d6dff','#33d17a','#62e89c','#ff5ea0','#ff8ec6'],
    ramp:   ['#2a1057','#4a2080','#7c4dff','#33d17a','#62e89c','#ff5ea0','#ff8ec6'],
    cycle:  ['#2a1057','#33d17a'] },
  { name: 'sunset',   // pink-purple horizon: rose→amber, warm/melancholy
    palette: ['#d9539c','#ff8e72','#ffd166','#a86bd1','#ef6f9c'],
    comps:  ['#3a1a4a','#6a2a7a','#a23aa8','#d9539c','#ef6f9c','#ff8e72','#ffae5a','#ffd166'],
    ramp:   ['#3a1a4a','#6a2a7a','#a23aa8','#d9539c','#ef6f9c','#ff8e72','#ffd166'],
    cycle:  ['#3a1a4a','#ffd166'] },
  { name: 'forest',   // woodsy: moss→bark→amber, earthen & calm
    palette: ['#4a7c3a','#8b9a46','#c4a34a','#6b4423','#a87f4a'],
    comps:  ['#2a3a1a','#3a522a','#4a7c3a','#5e8e4a','#8b9a46','#a8b04a','#c4a34a','#d4b25a'],
    ramp:   ['#2a3a1a','#3a522a','#4a7c3a','#8b9a46','#c4a34a','#6b4423','#a87f4a'],
    cycle:  ['#2a3a1a','#c4a34a'] },
  { name: 'candy',    // playful pastel: mint / peach / pink, bright & soft
    palette: ['#ff9ecb','#ffc485','#a8e6cf','#b5d8f0','#c2a8e6'],
    comps:  ['#f4a8d8','#ffb0a8','#ffc485','#ffe88a','#a8e6cf','#b5d8f0','#c2a8e6','#d8b5e6'],
    ramp:   ['#f4a8d8','#ffb0a8','#ffc485','#ffe88a','#a8e6cf','#b5d8f0','#d8b5e6'],
    cycle:  ['#f4a8d8','#a8e6cf'] },
];

// Per-mode swatch count. The Rust pal_len arg to build_trail_geometry and the
// mod-mask in the dot shader both rely on this matching the active palette
// set's array length. Comp-size / cycle-length / in-degree snippet their own
// length out of the named mode's set array (see PALETTES note above).
const PAL_LEN_BY_MODE = [5, 8, 7, 2, 7, 3, 8, 4, 7];
const palLen = (mode, set=palSet) => {
  // For modes 5/6/7 the JS-defined widths (3/4/4) are independent of the set;
  // they reuse ramps / comps / palette subslices — so always return the
  // fixed PAL_LEN_BY_MODE entry rather than the named array length.
  return PAL_LEN_BY_MODE[mode] || PALETTES[set].palette.length;
};

const SAGE = cssVar('sage'), CORAL = cssVar('coral'), RULE = cssVar('rule'), INK = cssVar('ink');

function hexToRgb(h){const n=parseInt(h.slice(1),16);return [(n>>16&255)/255,(n>>8&255)/255,(n&255)/255];}

// Build a (mode × set) cache of Float32Array(24) buffers. For modes 5/6/7 we
// synthesize the per-mode palette by slicing the set's comps/palette/ramp.
function modePalette(setIdx, mode){
  const s = PALETTES[setIdx];
  switch(mode){
    case 0: return s.palette;
    case 1: return s.comps;
    case 2: return s.ramp;
    case 3: return s.cycle;
    case 4: return s.ramp;                 // speed — ramp gradient
    case 5: return s.ramp.slice(0,3);     // comp-size — 3 bands
    case 6: return s.comps;                // cycle-length — one swatch per length (≤8)
    case 7: return s.palette.slice(0,4);  // in-degree — 4 buckets
    case 8: return s.ramp;                // birth-order — ramp gradient
    default: return s.palette;
  }
}
const palCache = PALETTES.map((_,setIdx)=>{
  return Array.from({length: 9}, (_,mode)=>{
    const pal = modePalette(setIdx, mode);
    const a = new Float32Array(24);
    pal.forEach((c,i)=>{const[r,g,b]=hexToRgb(c);a[i*3]=r;a[i*3+1]=g;a[i*3+2]=b;});
    return a;
  });
});
let palSet = 3; // default set = 'viridis'
function palFloats(mode, set=palSet){ return palCache[set][mode]; }

const FIELD = (()=>{const[r,g,b]=hexToRgb(cssVar('field'));return [r,g,b,1];})();
// Pre-parsed line colours — parsing hex strings every frame in buildLines is
// avoidable churn.
const RULE_RGB=hexToRgb(RULE),SAGE_RGB=hexToRgb(SAGE),CORAL_RGB=hexToRgb(CORAL);

const POSITION_INPUTS = `
layout(location=0) in vec4 aP0;
layout(location=1) in vec4 aP1;
layout(location=2) in vec4 aP2;
layout(location=3) in vec4 aP3;
layout(location=4) in vec4 aP4;
layout(location=5) in vec4 aP5;
void loadPosition(out float p[24]){
  p[0]=aP0.x;p[1]=aP0.y;p[2]=aP0.z;p[3]=aP0.w;
  p[4]=aP1.x;p[5]=aP1.y;p[6]=aP1.z;p[7]=aP1.w;
  p[8]=aP2.x;p[9]=aP2.y;p[10]=aP2.z;p[11]=aP2.w;
  p[12]=aP3.x;p[13]=aP3.y;p[14]=aP3.z;p[15]=aP3.w;
  p[16]=aP4.x;p[17]=aP4.y;p[18]=aP4.z;p[19]=aP4.w;
  p[20]=aP5.x;p[21]=aP5.y;p[22]=aP5.z;p[23]=aP5.w;
}
`;

// ---- shared GLSL: dynamic N-dimensional projection chain ----
// Emitted in two variants. `uTouring` is a uniform, so a single shader forces
// the compiler to keep the tour branch — and its dynamic `p[k]` indexing — live
// for every vertex, which pushes `float p[24]` into indexable scratch memory
// even in 3D. Splitting on it lets the non-tour variant keep the position in
// registers, and drops the 576 tour floats from the per-frame uniform upload.
const TOUR_PROJECTION = `
    ax=0.0;ay=0.0;az=0.0;
    for(int k=0;k<24;k++){
      if(float(k)>=uDim) break;
      ax+=uTourF[k]*p[k];
      ay+=uTourF[24+k]*p[k];
      az+=uTourF[48+k]*p[k];
    }
`;

const MANUAL_PROJECTION = `
    ax=x-uCx; ay=y-uCyC; az=z-uCz;
    if(uDim>3.5 && uDim<6.5){
      if(uIso>0.5){
        float c1=uIsoC,s1=uIsoS;
        float rx=ax*c1-ay*s1, ry=ax*s1+ay*c1;
        float rz=az*c1-aw*s1, rw=az*s1+aw*c1;
        ax=rx;ay=ry;az=rz;aw=rw;
      }
      float x1=ax*uCosA+aw*uSinA, w1=-ax*uSinA+aw*uCosA;
      float y1=ay*uCosB+w1*uSinB, w2=-ay*uSinB+w1*uCosB;
      ax=x1;ay=y1;aw=w2;
      if(uDim>4.5){
        float z1=az*uCosC+av*uSinC;
        av=-az*uSinC+av*uCosC;
        az=z1;
      }
      if(uWp>0.5){
        float denom=2.2+aw;
        float kk=2.2/max(denom,0.05);
        if(denom<=0.05){ok=false;}
        ax*=kk;ay*=kk;az*=kk;
      }
    }
`;

const TOUR_CULL = `
  float sq=0.0;
  for(int s=0;s<21;s++){
    if(float(s)>=uNslice) break;
    float q=s==0 ? -uSlabC : 0.0;
    for(int k=0;k<24;k++){
      if(float(k)>=uDim) break;
      q+=uTourN[s*24+k]*p[k];
    }
    sq+=q*q;
  }
  return sqrt(sq)>uSlabH;
`;

const MANUAL_CULL = `
  if(uDim>3.5){
    if(abs(p[3]-uSlabC)>uSlabH) return true;
    if(uDim>4.5 && abs(p[4]-uSlabC)>uSlabH) return true;
  }
  return false;
`;

const PROJ = touring => `
uniform float uSy,uCy,uSp,uCp,uDist,uFov,uHalf,uCx,uCyC,uCz;
uniform float uSinA,uCosA,uSinB,uCosB,uSinC,uCosC,uIsoC,uIsoS;
uniform float uSlabH,uSlabC,uTouring,uDim,uW,uH,uNslice,uFog,uWp,uIso,uBase,uRound;
${touring ? 'uniform float uTourF[72];\nuniform float uTourN[504];' : ''}

vec4 proj(float p[24], out float ps, out float pd, out bool ok){
  float x=p[0],y=p[1],z=p[2],w=p[3],v=p[4];
  float ax,ay,az,aw=w,av=v;
  ok=true; ps=1.0; pd=1.0;
${touring ? TOUR_PROJECTION : MANUAL_PROJECTION}
  float x1=ax*uCy+az*uSy;
  float z1=-ax*uSy+az*uCy;
  float y2=ay*uCp-z1*uSp;
  float z2=ay*uSp+z1*uCp;
  float d=uDist+z2;
  // Behind/too-close to camera: clamp d instead of teleporting to a corner.
  // A corner teleport drags a line segment across the whole canvas (the stray-
  // line bug). Clamping keeps the projected point finite and roughly in place;
  // the caller still sees ok=false and zeroes the alpha so nothing shows.
  if(d<0.02){ ok=false; d=0.02; }
  float zoom=uHalf/max(min(uW,uH)*0.36,1.0);
  ps=uFov/d*zoom; pd=d;
  float sx=uW*0.5+x1*ps;
  float sy=uH*0.5+y2*ps;
  return vec4(sx/uW*2.0-1.0, 1.0-sy/uH*2.0, 0.0, 1.0);
}

// slab slice test (tour: orthogonal-space; non-tour 4d/5d: |w|,|v| window)
bool sliceCull(float p[24]){
${touring ? TOUR_CULL : MANUAL_CULL}
}
`;

// ---- point shader (dots) ----
const DOT_VS = touring => `#version 300 es
precision highp float;
${POSITION_INPUTS}
layout(location=6) in float aCol;
${PROJ(touring)}
uniform vec3 uPal[8];
uniform float uOpacity;
out vec4 vColor;
void main(){
  float p[24];loadPosition(p);
  if(sliceCull(p)){gl_Position=vec4(2.0,2.0,2.0,1.0);gl_PointSize=0.0;vColor=vec4(0.0);return;}
  float ps,pd; bool ok;
  vec4 clip=proj(p,ps,pd,ok);
  if(!ok){gl_Position=vec4(2.0,2.0,2.0,1.0);gl_PointSize=0.0;vColor=vec4(0.0);return;}
  float ref=uFov/uDist;
  float r=clamp(uBase*ps/ref,0.5,7.0);
  float t=clamp((pd-(uDist*0.5))/uDist,0.0,1.0);
  float alpha=(uFog>0.5?(1.0-0.62*t):1.0)*uOpacity;
  gl_Position=clip;
  gl_PointSize=r*2.0;
  vColor=vec4(uPal[int(aCol+0.5)],alpha);
}`;

const DOT_FS = `#version 300 es
precision highp float;
in vec4 vColor;
uniform float uRoundU;
out vec4 outColor;
void main(){
  vec2 p=gl_PointCoord*2.0-1.0;
  if(uRoundU>0.5 && dot(p,p)>1.0) discard;
  outColor=vColor;
}`;

// ---- line shader (trails, links, marks, floor) ----
const LINE_VS = touring => `#version 300 es
precision highp float;
${POSITION_INPUTS}
layout(location=6) in vec4 aColA;
${PROJ(touring)}
uniform float uOpacity;
out vec4 vColor;
void main(){
  float p[24];loadPosition(p);
  float ps,pd; bool ok;
  // Always compute the real clip position; NEVER teleport culled verts (a
  // teleport drags a segment across the screen). Just zero the alpha so any
  // segment touching a culled vertex fades to invisible in place.
  bool culled=sliceCull(p);
  vec4 clip=proj(p,ps,pd,ok);
  if(!ok||culled){ gl_Position=clip; vColor=vec4(0.0,0.0,0.0,0.0); return; }
  gl_Position=clip;
  vColor=vec4(aColA.rgb,aColA.a*uOpacity);
}`;

const LINE_FS = `#version 300 es
precision highp float;
in vec4 vColor;
out vec4 outColor;
void main(){ outColor=vColor; }`;

// ---- GL boot ----
const E = i => document.getElementById(i);

async function loadBuildVersion(){
  try{
    const response=await fetch('./version.json',{cache:'no-store'});
    if(!response.ok)throw new Error(`HTTP ${response.status}`);
    const display=formatBuildVersion(await response.json());
    if(!display)throw new Error('invalid metadata');
    const element=E('app-version');
    element.textContent=display.label;
    element.title=display.description;
    element.setAttribute('aria-label',display.description);
    element.hidden=false;
  }catch(error){
    console.warn(`Build version unavailable: ${error.message}`);
  }
}

loadBuildVersion();
const cv = E('c');
let gl=null, dotProg=null, lineProg=null, flock=null, running=true;
let posBuf=null, colBuf=null, dotVao=null;
let trailVao=null, trailPosBuf=null, trailColBuf=null, trailIdxBuf=null;
let lineVao=null, linePosBuf=null, lineColBuf=null;
let roundU=1;

function compile(type,src){const s=gl.createShader(type);gl.shaderSource(s,src);gl.compileShader(s);
  if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw new Error('shader: '+gl.getShaderInfoLog(s)+'\n'+src);return s;}
function makeProgram(vs,fs){const p=gl.createProgram();
  gl.attachShader(p,compile(gl.VERTEX_SHADER,vs));gl.attachShader(p,compile(gl.FRAGMENT_SHADER,fs));
  gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw new Error('link: '+gl.getProgramInfoLog(p));return p;}

function setupGL(){
  gl=cv.getContext('webgl2',{antialias:true,alpha:false});
  if(!gl){E('legend').textContent='WebGL2 not available.';return false;}
  useVariant(false);
  dotVao=gl.createVertexArray(); trailVao=gl.createVertexArray(); lineVao=gl.createVertexArray();
  posBuf=gl.createBuffer(); colBuf=gl.createBuffer();
  trailPosBuf=gl.createBuffer(); trailColBuf=gl.createBuffer(); trailIdxBuf=gl.createBuffer();
  linePosBuf=gl.createBuffer(); lineColBuf=gl.createBuffer();
  gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);
  return true;
}

// Programs are compiled once per variant and kept; touring only toggles from
// the UI or on entering a high dimension.
const variants=new Map();
let activeVariant=null;
function useVariant(touring){
  const key=touring?1:0;
  let variant=variants.get(key);
  if(!variant){
    const dot=makeProgram(DOT_VS(touring),DOT_FS);
    const line=makeProgram(LINE_VS(touring),LINE_FS);
    variant={dot,line,setDot:buildUniformSetter(dot),setLine:buildUniformSetter(line)};
    variants.set(key,variant);
  }
  if(variant!==activeVariant){
    activeVariant=variant;
    dotProg=variant.dot; lineProg=variant.line;
    setDotUniforms=variant.setDot; setLineUniforms=variant.setLine;
  }
  return variant;
}

function size(){
  const d=Math.min(window.devicePixelRatio||1,2);
  const width=Math.max(1,Math.round(cv.clientWidth*d));
  const height=Math.max(1,Math.round(cv.clientHeight*d));
  if(cv.width!==width||cv.height!==height){
    cv.width=width;cv.height=height;
    if(gl)gl.viewport(0,0,width,height);
  }
}

// uniform locations are resolved once per program (getUniformLocation is not
// free — it used to run ~110x per frame) and dispatched through per-program
// setter closures built from the shared U[] layout.
const UNIF_NAMES=['uSy','uCy','uSp','uCp','uDist','uFov','uHalf','uCx','uCyC','uCz',
  'uSinA','uCosA','uSinB','uCosB','uSinC','uCosC','uIsoC','uIsoS','uSlabH','uSlabC',
  'uTouring','uDim','uW','uH','uNslice','uFog','uWp','uIso','uBase','uRound'];
const UNIF_ARRAYS=[['uTourF',30,72],['uTourN',102,504]];
const uniLocCache=new Map();
function U(prog,name){
  let m=uniLocCache.get(prog);
  if(!m){m=new Map();uniLocCache.set(prog,m);}
  let l=m.get(name);
  if(l===undefined){l=gl.getUniformLocation(prog,name);m.set(name,l);}
  return l;
}
function buildUniformSetter(prog){
  const scalars=UNIF_NAMES.map((name,i)=>({loc:U(prog,name),i}));
  const arrays=UNIF_ARRAYS.map(([name,off,len])=>({loc:U(prog,name+'[0]'),off,len}));
  return u=>{
    gl.useProgram(prog);
    for(const s of scalars)gl.uniform1f(s.loc,u[s.i]);
    for(const a of arrays)gl.uniform1fv(a.loc,u.subarray(a.off,a.off+a.len));
  };
}
let setDotUniforms=null,setLineUniforms=null;

let uniCache=null;
function refreshUniforms(){ uniCache=flock.uniforms(cv.width,cv.height); }

// ---- zero-copy views into wasm memory ----
// Rust owns persistent buffers; JS views them directly instead of copying via
// Float32Array.from(...) every frame. Views must be re-created whenever the
// backing memory could have moved (wasm memory growth, buffer realloc on
// set_n/set_dim) — tracked by ptr/len pairs + the memory buffer reference.
function wasmMem(){
  // The Memory object is stable for the module's lifetime; only its .buffer
  // changes, and only when wasm memory grows.
  if(!wasmMemory)wasmMemory=window.wasmBindings.wasm_memory();
  return wasmMemory;
}
// The backing ArrayBuffer only changes when wasm memory grows — cache the
// typed views on the buffer identity instead of allocating new ones per call.
let wasmMemory=null,memBuf=null,memF32=null,memU8=null,memI32=null,memU32=null;
function memViews(){
  const b=wasmMem().buffer;
  if(b!==memBuf){memBuf=b;memF32=new Float32Array(b);memU8=new Uint8Array(b);memI32=new Int32Array(b);memU32=new Uint32Array(b);}
}
function f32view(ptr,len){
  memViews();
  const o=ptr/4;
  if(o+len<=memF32.length)return memF32.subarray(o,o+len);
  return memF32.slice(o,o+len); // moved during the frame: fall back to a copy
}
function u8view(ptr,len){
  memViews();
  if(ptr+len<=memU8.length)return memU8.subarray(ptr,ptr+len);
  return memU8.slice(ptr,ptr+len);
}
function i32view(ptr,len){
  memViews();
  const o=ptr/4;
  if(o+len<=memI32.length)return memI32.subarray(o,o+len);
  return memI32.slice(o,o+len);
}
function u32view(ptr,len){
  memViews();
  const o=ptr/4;
  if(o+len<=memU32.length)return memU32.subarray(o,o+len);
  return memU32.slice(o,o+len);
}

let posView=null, colView=null, frView=null, enView=null;
let lastGraphVersion=-1;

function refreshViews(){
  posView=f32view(flock.positions_ptr(),flock.positions_len());
  colView=u8view(flock.colors_ptr(),flock.colors_len());
  frView=null;enView=null;lastGraphVersion=-1;
}
function graphViews(){
  if(lastGraphVersion!==flock.graph_version()){
    frView=i32view(flock.friends_ptr(),flock.graph_len());
    enView=i32view(flock.enemies_ptr(),flock.graph_len());
    lastGraphVersion=flock.graph_version();
  }
  return [frView,enView];
}

// ---- GL buffer helpers: size once, update with bufferSubData ----
function ensureBufferSize(buf,bytes){
  gl.bindBuffer(gl.ARRAY_BUFFER,buf);
  if((buf._size||0)<bytes){gl.bufferData(gl.ARRAY_BUFFER,bytes,gl.DYNAMIC_DRAW);buf._size=bytes;}
}

const MAX_POSITION_ATTRS=6;
function bindPositionAttributes(dim){
  const stride=dim*4;
  for(let attr=0;attr<MAX_POSITION_ATTRS;attr++){
    const offset=attr*4;
    if(offset<dim){
      gl.enableVertexAttribArray(attr);
      gl.vertexAttribPointer(attr,Math.min(4,dim-offset),gl.FLOAT,false,stride,offset*4);
    }else{
      gl.disableVertexAttribArray(attr);
      gl.vertexAttrib4f(attr,0,0,0,0);
    }
  }
}

function uploadDots(){
  posView=f32view(flock.positions_ptr(),flock.positions_len());
  gl.bindVertexArray(dotVao);
  ensureBufferSize(posBuf,posView.byteLength);
  gl.bufferSubData(gl.ARRAY_BUFFER,0,posView);
  // Vertex-attrib layout lives in the VAO and only depends on dim — rebind
  // (attrs 0-5 from posBuf, attr 6 from colBuf) only when dim changes.
  const dim=flock.dim();
  if(dotVao._dim!==dim){
    dotVao._dim=dim;
    bindPositionAttributes(dim);
    gl.bindBuffer(gl.ARRAY_BUFFER,colBuf);
    gl.enableVertexAttribArray(6); gl.vertexAttribPointer(6,1,gl.UNSIGNED_BYTE,false,0,0);
  }
  if(flock.sync_colors()||colView===null){
    colView=u8view(flock.colors_ptr(),flock.colors_len());
    ensureBufferSize(colBuf,colView.byteLength);
    gl.bufferSubData(gl.ARRAY_BUFFER,0,colView);
  }
  gl.bindVertexArray(null);
}

// ---- trails: independent GL_LINES segments from the ring buffer ----
// Strips would drag a streak to the corner when a vertex is culled behind the
// camera / outside the slab (the shader teleports culled verts offscreen, which
// does NOT break a strip). Independent segments let a culled point fade out in
// place. Rust emits one vertex per sample plus a segment index buffer, so the
// interior points are uploaded and transformed once rather than twice.
let trailQuality=1,trailEffective=1,trailCeiling=1,trailObserve=true,trailIndexVersion=-1;

function resetTrailAdaptation(resetQuality){
  if(resetQuality)trailQuality=1;
  trailObserve=true;
  fpsWindowStart=0;fpsFrames=0;
}

function trailBudget(){
  const {selected,effective,ceiling}=calculateTrailBudget(flock.n(),trailQuality,flock.trail_slots(),flock.dim());
  trailEffective=effective;
  trailCeiling=ceiling;
  return selected;
}

function buildTrails(){
  if(!P.trails){trailEffective=0;return 0;}
  const count=flock.build_trail_geometry(palFloats(colMode),palLen(colMode),trailBudget());
  if(count===0) return 0;
  const dim=flock.dim();
  const verts=f32view(flock.trail_verts_ptr(),count*dim);
  const vc=f32view(flock.trail_cols_ptr(),count*4);
  gl.bindVertexArray(trailVao);
  ensureBufferSize(trailPosBuf,verts.byteLength);
  gl.bufferSubData(gl.ARRAY_BUFFER,0,verts);
  if(trailVao._dim!==dim){
    trailVao._dim=dim;
    bindPositionAttributes(dim);
    gl.bindBuffer(gl.ARRAY_BUFFER,trailColBuf);
    gl.enableVertexAttribArray(6);gl.vertexAttribPointer(6,4,gl.FLOAT,false,0,0);
  }
  ensureBufferSize(trailColBuf,vc.byteLength);
  gl.bufferSubData(gl.ARRAY_BUFFER,0,vc);
  // The segment list only changes when the trail count or depth does, so it is
  // uploaded on a version bump rather than every frame.
  const indexCount=flock.trail_index_count();
  const version=flock.trail_index_version();
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,trailIdxBuf);
  if(trailIndexVersion!==version){
    trailIndexVersion=version;
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,u32view(flock.trail_indices_ptr(),indexCount),gl.STATIC_DRAW);
  }
  gl.bindVertexArray(null);
  return indexCount;
}

// ---- links + marks + floor (per-frame line geometry) ----
// Preallocated scratch to avoid per-frame GC churn (the old .push version was
// the 5fps bottleneck at n=10000). Links subsample like trails at high n.
let lineVertsScratch=null, lineColsScratch=null;
const NO_LINES={verts:new Float32Array(0),cols:new Float32Array(0),count:0};
function buildLines(){
  if(!P.links && !(P.shadow&&dimVal===3))return NO_LINES;
  const [rr,rg,rb]=RULE_RGB;
  const [sr,sg,sb]=SAGE_RGB;
  const [cr,cg,cb]=CORAL_RGB;
  const n=flock.n();
  const dim=flock.dim();

  // links subsample so n=10000 doesn't push 20k edges through the CPU
  const linkStride=Math.ceil(n/1200);
  const linkEdges=P.links?Math.ceil(n/linkStride)*2:0;
  const floorEdges=(P.shadow&&dimVal===3)?20:0;
  const totalVerts=linkEdges*2+floorEdges*2;

  if(!lineVertsScratch||lineVertsScratch.length<totalVerts*dim){
    lineVertsScratch=new Float32Array(Math.max(1,totalVerts)*dim);
    lineColsScratch=new Float32Array(Math.max(1,totalVerts)*4);
  }
  const verts=lineVertsScratch, vc=lineColsScratch;
  let vp=0,cp=0;
  const pushFrom=(positions,offset,r,g,b,a)=>{
    for(let k=0;k<dim;k++)verts[vp+k]=positions[offset+k];
    vp+=dim;
    vc[cp]=r;vc[cp+1]=g;vc[cp+2]=b;vc[cp+3]=a;cp+=4;
  };
  const pushFloor=(x,y,z,r,g,b,a)=>{
    verts[vp]=x;verts[vp+1]=y;verts[vp+2]=z;vp+=dim;
    vc[cp]=r;vc[cp+1]=g;vc[cp+2]=b;vc[cp+3]=a;cp+=4;
  };

  if(P.links){
    const [fr,en]=graphViews();
    const pos=posView;
    const la=n>2000?0.05:n>500?0.09:0.16;
    for(let i=0;i<n;i+=linkStride){
      const f=fr[i], e=en[i];
      const io=i*dim, fo=f*dim, eo=e*dim;
      pushFrom(pos,io,sr,sg,sb,la);pushFrom(pos,fo,sr,sg,sb,la);
      pushFrom(pos,io,cr,cg,cb,la);pushFrom(pos,eo,cr,cg,cb,la);
    }
  }

  if(P.shadow && dimVal===3){
    const ga=0.35;
    for(let g=-2;g<=2;g++){
      const u=g*0.25;
      pushFloor(u,0.5,-0.5,rr,rg,rb,ga);pushFloor(u,0.5,0.5,rr,rg,rb,ga);
      pushFloor(-0.5,0.5,u,rr,rg,rb,ga);pushFloor(0.5,0.5,u,rr,rg,rb,ga);
    }
  }
  return {verts:verts.subarray(0,vp), cols:vc.subarray(0,cp), count:vp/dim};
}

function uploadLines(L){
  gl.bindVertexArray(lineVao);
  ensureBufferSize(linePosBuf,L.verts.byteLength);
  gl.bufferSubData(gl.ARRAY_BUFFER,0,L.verts);
  const dim=flock.dim();
  if(lineVao._dim!==dim){
    lineVao._dim=dim;
    bindPositionAttributes(dim);
    gl.bindBuffer(gl.ARRAY_BUFFER,lineColBuf);
    gl.enableVertexAttribArray(6);gl.vertexAttribPointer(6,4,gl.FLOAT,false,0,0);
  }
  ensureBufferSize(lineColBuf,L.cols.byteLength);
  gl.bufferSubData(gl.ARRAY_BUFFER,0,L.cols);
  gl.bindVertexArray(null);
}

function draw(){
  gl.clearColor(FIELD[0],FIELD[1],FIELD[2],1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  useVariant(flock.touring_active());
  refreshUniforms();

  // trails + links/floor share the line program and uniform values — push
  // them once per frame instead of per draw batch.
  const tc=buildTrails();
  const L=buildLines();
  if(tc>0||L.count>0){
    setLineUniforms(uniCache);
    gl.uniform1f(U(lineProg,'uOpacity'),P.opacity);
  }

  // trails first (under dots): independent segments
  if(tc>0){
    gl.bindVertexArray(trailVao);
    gl.drawElements(gl.LINES,tc,gl.UNSIGNED_INT,0);
    gl.bindVertexArray(null);
  }

  // links + marks + floor
  if(L.count>0){
    uploadLines(L);
    gl.bindVertexArray(lineVao);
    gl.drawArrays(gl.LINES,0,L.count);
    gl.bindVertexArray(null);
  }

  // dots on top
  setDotUniforms(uniCache);
  gl.uniform3fv(U(dotProg,'uPal'),palFloats(colMode));
  gl.uniform1f(U(dotProg,'uRoundU'),roundU);
  gl.uniform1f(U(dotProg,'uOpacity'),P.opacity);
  gl.bindVertexArray(dotVao);
  gl.drawArrays(gl.POINTS,0,flock.n());
  gl.bindVertexArray(null);
}

// ---- readouts ----
let prevSpread=0,acc=0,fpsFrames=0,fpsWindowStart=0,measuredFps=0;
function updateFrameRate(now){
  if(!fpsWindowStart){fpsWindowStart=now;fpsFrames=0;return;}
  fpsFrames++;
  const elapsed=now-fpsWindowStart;
  if(elapsed<1000)return;
  measuredFps=fpsFrames*1000/elapsed;
  fpsWindowStart=now;fpsFrames=0;
  if(!P.trails)return;
  if(trailObserve){trailObserve=false;return;}
  trailQuality=nextTrailQuality(trailQuality,trailEffective,measuredFps,flock.n(),trailCeiling);
}
function readout(){
  E('r-step').textContent=Number(flock.steps()).toLocaleString();
  flock.measure_spread(); // full n*dim pass — runs on the readout cadence, not per frame
  const s=flock.spread();
  E('r-spread').textContent=s<1e4?s.toFixed(3):s.toExponential(1);
  E('r-view').textContent=flock.yaw_deg()+'° / '+flock.pitch_deg()+'°';
  E('r-space').textContent=dimVal+'d';
  E('r-leg').textContent=flock.touring_active()?flock.tour_leg()+' ('+Math.round(flock.tour_t()*100)+'%)':'—';
  const g=s/(prevSpread||1e-12);
  let v='holding';
  if(s<1e-3)v='collapsed';else if(g>1.02)v='expanding';else if(g<0.98)v='contracting';
  E('r-verdict').textContent=v;
  prevSpread=s;
  E('r-fps').textContent=measuredFps?Math.round(measuredFps):'—';
  E('r-trails').textContent=trailQualityLabel(P.trails,trailEffective);
}

const swatches=(arr,caps)=>arr.map((c,i)=>'<span class="key"><i style="background:'+c+'"></i>'+caps[i]+'</span>').join('');
function legend(){
  const g=E('legend');
  const pal = modePalette(palSet, colMode);
  if(colMode===1){flock.analyse();g.innerHTML='One colour per component of the friend graph. Now: <b>'+flock.ncomp()+'</b>.';}
  else if(colMode===2){flock.analyse();g.innerHTML=swatches([pal[0],pal[pal.length-1]],['on the cycle','deepest tree'])+'Friend-steps to the cycle, square-root scale. Longest chain now: <b>'+flock.dmax()+'</b>.';}
  else if(colMode===3){flock.analyse();g.innerHTML=swatches(pal,['in a tree','on a cycle'])+'Cyclic dots have nowhere to drain to. Now: <b>'+flock.on_cycle_count()+'</b> of '+flock.n()+'.';}
  else if(colMode===4){g.innerHTML=swatches([pal[0],pal[pal.length-1]],['slowest','fastest'])+'Per-frame motion, square-root scale. Frame max Δ²: <b>'+(flock.spd_max()||0).toFixed(4)+'</b>.';}
  else if(colMode===5){flock.analyse();g.innerHTML=swatches(pal,['small','medium','large'])+'Components bucketed by node count. Largest component now: <b>'+flock.comp_size_max()+'</b>.';}
  else if(colMode===6){flock.analyse();g.innerHTML=swatches(pal,['L2','L3','L4','L5','L6','L7','L8','≥9'])+'Cycle length per component — each length has its own colour. Longest cycle now: <b>'+flock.cyc_len_max()+'</b>.';}
  else if(colMode===7){flock.analyse();g.innerHTML=swatches(pal,['unloved','casual','popular','hub'])+'In-degree bucket of the friend graph. Max degree bucket now: <b>'+flock.ind_max()+'</b>.';}
  else if(colMode===8){g.innerHTML=swatches([pal[0],pal[Math.floor(pal.length/2)],pal[pal.length-1]],['first','mid','last'])+'Birth order — dot index as a gradient, so you can see mixing.';}
  else g.innerHTML='Colour carries nothing here.';
}

// ---- state ----
const P={trails:true,links:false,shadow:true,opacity:0.5};
let dimVal=3, colMode=0, speed=1;

// Slider value → dot count. Exponential mapping so 0..1000 covers 3..1,000,000.
const dotsFor=populationForSliderValue;

function labels(){
  const unit=!lawProp;
  E('v-friend').textContent=unit?(+E('s-friend').value/1000).toFixed(3)+' w':(+E('s-friend').value).toFixed(1)+'% of gap';
  E('v-enemy').textContent=unit?(+E('s-enemy').value/1000).toFixed(3)+' w':(+E('s-enemy').value).toFixed(1)+'% of gap';
  E('v-centre').textContent=(+E('s-centre').value/10).toFixed(1)+'% of gap';
  E('v-rate').textContent='~'+E('s-rate').value+' steps';
  E('v-speed').textContent=E('s-speed').value;
  const lens=+E('s-lens').value/100;
  E('v-lens').textContent=lens<0.35?'wide':lens>0.9?'long':'normal';
  E('v-wa').textContent=E('s-wa').value+'°';
  E('v-wb').textContent=E('s-wb').value+'°';
  E('v-wc').textContent=E('s-wc').value+'°';
  E('v-sc').textContent=(+E('s-sc').value/100).toFixed(2);
  E('v-sw').textContent=(+E('s-sw').value/100)>=2?'everything':'±'+(+E('s-sw').value/100).toFixed(2);
  E('v-leg').textContent=E('s-leg').value+'s';
  E('v-trail-length').textContent=E('s-trail-length').value+' frames';
  E('v-opacity').textContent=E('s-opacity').value+'%';
  E('law-note').textContent=lawProp
    ?'Step size scales with distance, exactly as written. The update is then linear, so a run either collapses to the centre or grows without bound.'
    :'Every step is the same length whatever the distance. Non-linear, so the cloud can stay bounded and churn indefinitely.';
  E('tour-note').textContent=E('k-tour').checked
    ?'Touring: the view walks geodesics through the space of 3-planes, so every projection turns up eventually. Structure that survives many legs is real. The slab now cuts a slice parallel to the plane you are looking at.'
    :'';
}

let lawProp=false;
function setLaw(prop){
  lawProp=prop;
  E('m-prop').setAttribute('aria-pressed',prop);
  E('m-unit').setAttribute('aria-pressed',!prop);
  flock.set_law_prop(prop);
  labels();
}

function setDim(d){
  dimVal=d;
  for(const value of [2,3,4,5,8,24])E('d-'+value).setAttribute('aria-pressed',value===d);
  const high=d>5;
  E('wgroup').style.display=d>=4&&!high?'flex':'none';
  E('vrow').style.display=d>=5&&!high?'flex':'none';
  E('low-dim-note').hidden=high;
  E('high-dim-note').hidden=!high;
  const dots=E('s-dots');
  const maxPosition=maxPopulationSliderValue(+E('s-trail-length').value,d);
  const populationClamped=+dots.value>maxPosition;
  if(populationClamped)dots.value=maxPosition;
  flock.set_dim(d);
  if(populationClamped)flock.set_n(dotsFor(+dots.value));
  if(high){E('k-tour').checked=true;flock.set_tour(true);}
  refreshViews();
  uploadDots();
  E('v-dots').textContent=flock.n();
  if(d===2){E('k-spin').checked=false;flock.set_spin(false);}
  labels();legend();
}

// ---- loop ----
function loop(now=performance.now()){
  if(running){
    for(let s=0;s<speed;s++)flock.step();
    // Recording while trails are hidden costs a full n*dim copy per frame and
    // nothing reads it.
    if(P.trails)flock.capture_trail_frame();
    uploadDots();
  }
  flock.update_camera(cv.width,cv.height);
  draw();
  updateFrameRate(now);
  if(++acc%6===0){readout();if(colMode!==0)legend();}
  requestAnimationFrame(loop);
}

// ---- controls ----
function wire(){
  const bind=(id,fn)=>E(id).addEventListener('input',e=>{fn(+e.target.value);labels();});
  bind('s-friend',v=>flock.set_friend(v/1000));
  bind('s-enemy',v=>flock.set_enemy(v/1000));
  bind('s-centre',v=>flock.set_centre(v/1000));
  bind('s-rate',v=>flock.set_repick(v));
  bind('s-speed',v=>{speed=v;flock.set_speed(v);});
  bind('s-leg',v=>flock.set_leg(v));
  bind('s-lens',v=>flock.set_lens(v/100));
  bind('s-opacity',v=>{P.opacity=v/100;});
  const rad=v=>v*Math.PI/180;
  bind('s-wa',v=>flock.set_wa(rad(v)));
  bind('s-wb',v=>flock.set_wb(rad(v)));
  bind('s-wc',v=>flock.set_wc(rad(v)));
  bind('s-sc',v=>flock.set_slab_c(v/100));
  bind('s-sw',v=>flock.set_slab_h(v/100));

  let dotsTimer=0,trailLengthTimer=0;
  E('s-dots').addEventListener('input',e=>{
    const maxPosition=maxPopulationSliderValue(+E('s-trail-length').value,dimVal);
    if(+e.target.value>maxPosition)e.target.value=maxPosition;
    const want=dotsFor(+e.target.value);
    E('v-dots').textContent=want;
    clearTimeout(dotsTimer);
    dotsTimer=setTimeout(()=>{
      flock.set_n(want);
      E('v-dots').textContent=flock.n();
      refreshViews();uploadDots();resetTrailAdaptation(true);
    },90);
  });

  E('s-trail-length').addEventListener('input',e=>{
    const length=+e.target.value;
    const dots=E('s-dots');
    const maxPosition=maxPopulationSliderValue(length,dimVal);
    if(+dots.value>maxPosition){
      dots.value=maxPosition;
      E('v-dots').textContent=dotsFor(maxPosition);
    }
    labels();
    clearTimeout(dotsTimer);
    clearTimeout(trailLengthTimer);
    trailLengthTimer=setTimeout(()=>{
      const safePopulation=maxPopulationForTrailLength(length,dimVal);
      if(flock.n()>safePopulation){
        flock.set_n(dotsFor(+dots.value));
      }
      flock.set_trail_length(length);
      E('v-dots').textContent=flock.n();
      refreshViews();uploadDots();resetTrailAdaptation(true);
    },90);
  });

  E('m-prop').onclick=()=>setLaw(true);
  E('m-unit').onclick=()=>setLaw(false);
  E('d-2').onclick=()=>setDim(2);
  E('d-3').onclick=()=>setDim(3);
  E('d-4').onclick=()=>setDim(4);
  E('d-5').onclick=()=>setDim(5);
  E('d-8').onclick=()=>setDim(8);
  E('d-24').onclick=()=>setDim(24);

  E('k-fit').onchange=e=>flock.set_fit(e.target.checked);
  E('k-trail').onchange=e=>{P.trails=e.target.checked;if(e.target.checked)flock.reset_trails();resetTrailAdaptation(e.target.checked);readout();};
  E('k-links').onchange=e=>P.links=e.target.checked;
  E('k-spin').onchange=e=>flock.set_spin(e.target.checked);
  E('k-round').onchange=e=>{flock.set_round(e.target.checked);roundU=e.target.checked?1:0;};
  E('k-fog').onchange=e=>flock.set_fog(e.target.checked);
  E('k-shadow').onchange=e=>P.shadow=e.target.checked;
  E('k-rock').onchange=e=>flock.set_rock(e.target.checked);
  E('k-iso').onchange=e=>flock.set_iso(e.target.checked);
  E('k-wp').onchange=e=>flock.set_wp(e.target.checked);
  E('k-tour').onchange=e=>{flock.set_tour(e.target.checked);labels();};
  E('s-colour').onchange=e=>{colMode=+e.target.value;flock.set_col_mode(colMode);uploadDots();legend();};
  E('s-palette').onchange=e=>{palSet=+e.target.value;uploadDots();legend();};

  const setRunning=r=>{running=r;flock.set_running(r);E('b-play').textContent=r?'Pause':'Play';};
  E('b-play').onclick=()=>setRunning(!running);
  E('b-repick').onclick=()=>flock.repick_all();
  E('b-reset').onclick=()=>flock.reset();
  E('b-view').onclick=()=>flock.reset_view();

  addEventListener('keydown',e=>{
    if(e.target.tagName==='INPUT'||e.target.tagName==='SELECT')return;
    if(e.code==='Space'){e.preventDefault();setRunning(!running);}
    if(e.key==='r')flock.reset();
    if(e.key==='R')flock.repick_all();
  });

  const tg=E('toggle');
  const togglePanel=()=>{
    const panel=E('panel');
    const open=panel.dataset.open==='true';
    if(open&&panel.contains(document.activeElement))tg.focus();
    panel.dataset.open=String(!open);
    tg.setAttribute('aria-expanded',String(!open));
    tg.textContent=open?'Rules':'Hide';
    size();
  };
  tg.onclick=togglePanel;
  addEventListener('keydown',e=>{
    if(e.target.tagName==='INPUT'||e.target.tagName==='SELECT')return;
    if(e.key==='h'||e.key==='H')togglePanel();
  });

  // One pointer orbits; two pointers pinch to zoom.
  const pointers=new Map();
  let pinchDistance=0;
  const distance=()=>{
    const [a,b]=[...pointers.values()];
    return a&&b?Math.hypot(a.x-b.x,a.y-b.y):0;
  };
  cv.addEventListener('pointerdown',e=>{
    pointers.set(e.pointerId,{x:e.clientX,y:e.clientY});
    if(pointers.size===2)pinchDistance=distance();
    cv.classList.add('dragging');
    cv.setPointerCapture(e.pointerId);
  });
  cv.addEventListener('pointermove',e=>{
    const previous=pointers.get(e.pointerId);
    if(!previous)return;
    pointers.set(e.pointerId,{x:e.clientX,y:e.clientY});
    if(pointers.size>=2){
      const nextDistance=distance();
      if(pinchDistance>0)flock.set_zoom(flock.zoom()*nextDistance/pinchDistance);
      pinchDistance=nextDistance;
    }else{
      flock.orbit(e.clientX-previous.x,e.clientY-previous.y);
    }
  });
  const releasePointer=e=>{
    pointers.delete(e.pointerId);
    pinchDistance=0;
    if(pointers.size===0)cv.classList.remove('dragging');
    try{cv.releasePointerCapture(e.pointerId);}catch(_){}
  };
  cv.addEventListener('pointerup',releasePointer);
  cv.addEventListener('pointercancel',releasePointer);
  cv.addEventListener('wheel',e=>{
    e.preventDefault();
    flock.set_zoom(flock.zoom()*Math.exp(-e.deltaY*0.0012));
  },{passive:false});
  if('ResizeObserver' in window)new ResizeObserver(size).observe(E('stage'));
  addEventListener('resize',size);
  document.addEventListener('visibilitychange',()=>{
    if(!document.hidden){resetTrailAdaptation(false);}
    else{fpsWindowStart=0;fpsFrames=0;}
  });
}

function start(){
  if(!setupGL())return;
  flock=new window.wasmBindings.Flock(dotsFor(+E('s-dots').value),3,BigInt(Date.now())>>8n);
  flock.sync_colors();
  refreshViews();
  // sync defaults from sliders
  flock.set_friend(+E('s-friend').value/1000);
  flock.set_enemy(+E('s-enemy').value/1000);
  flock.set_centre(+E('s-centre').value/1000);
  flock.set_repick(+E('s-rate').value);
  flock.set_speed(+E('s-speed').value);
  speed=+E('s-speed').value;
  flock.set_trail_length(+E('s-trail-length').value);
  flock.set_lens(+E('s-lens').value/100);
  flock.set_slab_h(+E('s-sw').value/100);
  size();
  // default colour mode = steps to its cycle (matches the selected option)
  colMode=2;
  flock.set_col_mode(2);
  uploadDots();
  wire();
  setLaw(false);
  setDim(3);
  labels();legend();readout();
  if(matchMedia('(prefers-reduced-motion: reduce)').matches){running=false;flock.set_running(false);E('b-play').textContent='Play';}
  // Seams for perf-harness.mjs; the module scope is otherwise unreachable.
  installPerfHarness({
    get flock(){return flock;},
    get gl(){return gl;},
    uploadDots,buildTrails,buildLines,uploadLines,draw,refreshUniforms,
    isRunning:()=>running,
    setRunning:r=>{running=r;flock.set_running(r);E('b-play').textContent=r?'Pause':'Play';},
    trailsEnabled:()=>P.trails,
  });
  loop();
}

if(window.wasmBindings)start();
else addEventListener('TrunkApplicationStarted',()=>start(),{once:true});
