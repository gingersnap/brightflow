/// The tracking script served to browsers.
///
/// This script is ~1200 bytes minified. It:
/// - Sends a pageview on load
/// - Tracks SPA navigation via `history.pushState` and `popstate`
/// - Exposes `window.brightflow.track(name, props)` for custom events
/// - Exposes `window.brightflow.identify(userId, traits)` for user identification
/// - Uses `navigator.sendBeacon` (fallback: XHR) for reliable delivery
/// - Reads `data-domain` from the script tag for source identification
/// - No cookies, no local storage, no fingerprinting
pub const TRACKING_SCRIPT: &str = r"(function(){
  'use strict';
  var s=document.currentScript;
  var domain=s&&s.getAttribute('data-domain')||'';
  var base=s&&s.src?s.src.replace(/\/api\/script\.js.*/,''):'';
  if(!base)return;
  var eventEndpoint=base+'/api/event';
  var identifyEndpoint=base+'/api/identify';
  var _uid='';

  function post(url,data){
    var body=JSON.stringify(data);
    if(navigator.sendBeacon){
      navigator.sendBeacon(url,body);
    }else{
      var xhr=new XMLHttpRequest();
      xhr.open('POST',url,true);
      xhr.setRequestHeader('Content-Type','text/plain');
      xhr.send(body);
    }
  }

  function send(name,props){
    var data={name:name,url:location.href,referrer:document.referrer||'',screenWidth:window.innerWidth,domain:domain};
    if(_uid)data.userId=_uid;
    if(props)data.props=props;
    post(eventEndpoint,data);
  }

  function identify(userId,traits){
    if(!userId)return;
    _uid=userId;
    var data={userId:userId,domain:domain};
    if(traits)data.traits=traits;
    post(identifyEndpoint,data);
  }

  var lastUrl=location.href;
  function trackIfNew(){
    if(location.href!==lastUrl){
      lastUrl=location.href;
      send('pageview');
    }
  }

  send('pageview');

  var orig=history.pushState;
  history.pushState=function(){
    orig.apply(history,arguments);
    trackIfNew();
  };
  var origReplace=history.replaceState;
  history.replaceState=function(){
    origReplace.apply(history,arguments);
    trackIfNew();
  };
  window.addEventListener('popstate',function(){trackIfNew()});

  window.brightflow={track:send,identify:identify};
})();";
