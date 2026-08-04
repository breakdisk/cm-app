/**
 * GrayscaleProcessor — on-device grayscale conversion via an offscreen WebView canvas.
 *
 * Why a WebView?  expo-image-manipulator has no grayscale action.  expo-gl would
 * require a mounted component anyway.  WKWebView (iOS) and Chromium WebView
 * (Android) both support the 2D canvas API and run the pixel loop off the
 * main thread inside the WebView renderer process.
 *
 * Pipeline the WebView handles:
 *   1. Decode base64 image into an <img> element.
 *   2. Draw onto a canvas, scaling down to ≤ 1280 px in the same drawImage()
 *      call (GPU-accelerated blit, avoids a full-resolution pixel loop).
 *   3. Iterate pixels with ITU-R BT.601 luma: g = (R×77 + G×150 + B×29) >> 8
 *      (integer shift avoids float overhead in the hot loop).
 *   4. Export as JPEG 92 % (data URI → base64 slice).
 *
 * Usage (mount once in your screen, call via ref):
 *   const grayscaleRef = useRef<GrayscaleProcessorRef>(null);
 *   // ...
 *   <GrayscaleProcessor ref={grayscaleRef} />
 *   // ...
 *   const grayBase64 = await grayscaleRef.current?.process(fileUri);
 */
import React, {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useRef,
} from 'react';
import { View } from 'react-native';
import WebView, { WebViewMessageEvent } from 'react-native-webview';
import { File } from 'expo-file-system';

export interface GrayscaleProcessorRef {
  /**
   * Read the image at `uri`, apply grayscale + resize to ≤ 1280 px, and
   * return the result as a base64 JPEG string (no data-URI prefix).
   * Rejects after 15 s if the WebView does not respond.
   */
  process(uri: string): Promise<string>;
}

const MAX_DIM = 1280;
const PROCESSOR_TIMEOUT_MS = 15_000;

const PROCESSOR_HTML = `<!DOCTYPE html>
<html><body style="margin:0">
<canvas id="c"></canvas>
<script>
var MAX=${MAX_DIM};
window.applyGrayscale=function(b64,mime){
  var img=new Image();
  img.onload=function(){
    var w=img.naturalWidth,h=img.naturalHeight;
    if(w>MAX||h>MAX){
      if(w>=h){h=Math.round(h*MAX/w);w=MAX;}
      else{w=Math.round(w*MAX/h);h=MAX;}
    }
    var c=document.getElementById('c');
    c.width=w;c.height=h;
    var ctx=c.getContext('2d');
    ctx.drawImage(img,0,0,w,h);
    var d=ctx.getImageData(0,0,w,h);
    var px=d.data;
    for(var i=0;i<px.length;i+=4){
      var g=(px[i]*77+px[i+1]*150+px[i+2]*29)>>8;
      px[i]=px[i+1]=px[i+2]=g;
    }
    ctx.putImageData(d,0,0);
    window.ReactNativeWebView.postMessage(c.toDataURL('image/jpeg',0.92).split(',')[1]);
  };
  img.onerror=function(){window.ReactNativeWebView.postMessage('ERROR');};
  img.src='data:'+mime+';base64,'+b64;
};
</script>
</body></html>`;

type Resolver = { resolve: (v: string) => void; reject: (e: Error) => void };

const GrayscaleProcessor = forwardRef<GrayscaleProcessorRef>((_, ref) => {
  const webViewRef = useRef<WebView>(null);
  const pending = useRef<Resolver | null>(null);

  const onMessage = useCallback((event: WebViewMessageEvent) => {
    const msg = event.nativeEvent.data;
    const p = pending.current;
    pending.current = null;
    if (!p) return;
    if (msg === 'ERROR') {
      p.reject(new Error('Grayscale canvas processing failed'));
    } else {
      p.resolve(msg);
    }
  }, []);

  useImperativeHandle(ref, () => ({
    async process(uri: string): Promise<string> {
      const file = new File(uri);
      // `extension` includes the leading dot, e.g. '.png'
      const mime = file.extension.toLowerCase() === '.png' ? 'image/png' : 'image/jpeg';

      const b64 = await file.base64();

      return new Promise<string>((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.current = null;
          reject(new Error('GrayscaleProcessor timed out'));
        }, PROCESSOR_TIMEOUT_MS);

        pending.current = {
          resolve: (v) => { clearTimeout(timer); resolve(v); },
          reject:  (e) => { clearTimeout(timer); reject(e);  },
        };

        // JSON.stringify safely escapes the base64 string as a JS string literal.
        const script = `window.applyGrayscale(${JSON.stringify(b64)},${JSON.stringify(mime)});true;`;
        webViewRef.current?.injectJavaScript(script);
      });
    },
  }), []);

  // Render off-screen: must stay mounted (not display:none) so the WebView
  // JS context is live, but invisible and zero-impact on layout.
  return (
    <View
      style={{
        position: 'absolute',
        width: 1,
        height: 1,
        top: -9999,
        left: -9999,
        overflow: 'hidden',
      }}
    >
      <WebView
        ref={webViewRef}
        source={{ html: PROCESSOR_HTML }}
        onMessage={onMessage}
        javaScriptEnabled
        originWhitelist={['*']}
        style={{ width: 1, height: 1 }}
      />
    </View>
  );
});

GrayscaleProcessor.displayName = 'GrayscaleProcessor';
export default GrayscaleProcessor;
