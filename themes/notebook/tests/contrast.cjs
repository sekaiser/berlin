// WCAG relative-luminance ratio for browser-computed RGB colors.
function contrast(first, second) {
  const luminance = color => {
    const [r,g,b] = color.match(/[\d.]+/g).slice(0,3).map(Number).map(value => {
      const channel=value/255;
      return channel<=.04045 ? channel/12.92 : ((channel+.055)/1.055)**2.4;
    });
    return r*.2126+g*.7152+b*.0722;
  };
  const values=[luminance(first),luminance(second)].sort((a,b)=>b-a);
  return (values[0]+.05)/(values[1]+.05);
}

module.exports={contrast};
