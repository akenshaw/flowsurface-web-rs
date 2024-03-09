//mod utils;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::RwLockReadGuard;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, MessageEvent, WebSocket};
use serde_json::Value;

extern crate js_sys;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

struct Trade {
    price: f64,
    quantity: f64,
    time: u64,
    is_buyer_maker: bool,
}

pub struct Bid {
    price: f64,
    quantity: f64,
}
pub struct Ask {
    price: f64,
    quantity: f64,
}
#[derive(Debug)]
pub struct Kline {
    open_time: u64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    buy_volume: f64,
    sell_volume: f64,
    close_time: u64,
}

#[wasm_bindgen]
pub struct CanvasManager {
    klines_ohlcv: Arc<RwLock<BTreeMap<u64, Kline>>>,
    klines_trades: Arc<RwLock<BTreeMap<u64, HashMap<(i64, bool), f64>>>>,
    bids: Arc<RwLock<Vec<Bid>>>,
    asks: Arc<RwLock<Vec<Ask>>>,
    canvas_main: CanvasMain,
    canvas_orderbook: CanvasOrderbook,
    canvas_indicator_volume: CanvasIndicatorVolume,
}

#[wasm_bindgen]
impl CanvasManager {
    pub fn new(canvas1: HtmlCanvasElement, canvas1_overlay: HtmlCanvasElement, canvas2: HtmlCanvasElement, canvas2_overlay: HtmlCanvasElement, canvas3: HtmlCanvasElement, canvas4: HtmlCanvasElement) -> Self {
        Self {
            klines_ohlcv: Arc::new(RwLock::new(BTreeMap::new())),
            klines_trades: Arc::new(RwLock::new(BTreeMap::new())),
            bids: Arc::new(RwLock::new(Vec::new())),
            asks: Arc::new(RwLock::new(Vec::new())),
            canvas_main: CanvasMain::new(canvas1).expect("Failed to create CanvasMain"),
            canvas_orderbook: CanvasOrderbook::new(canvas2).expect("Failed to create CanvasOrderbook"),
            canvas_indicator_volume: CanvasIndicatorVolume::new(canvas3).expect("Failed to create CanvasIndicatorVolume"),
        }
    }

    pub fn start_websocket(&mut self) {
        log(&format!("start websocket"));

        let ws = WebSocket::new("wss://fstream.binance.com/stream?streams=btcusdt@aggTrade/btcusdt@depth20@100ms/btcusdt@kline_1m").unwrap();

        let mut current_kline_open: u64 = 0;
        let mut trades_buffer: Vec<Trade> = Vec::new();
        let bucket_size = 5.0; 

        let klines_trades = Arc::clone(&self.klines_trades);
        let klines_ohlcv = Arc::clone(&self.klines_ohlcv);

        let bids = Arc::clone(&self.bids);
        let asks = Arc::clone(&self.asks);

        let onmessage_callback = Closure::wrap(Box::new(move |event: MessageEvent| {
            if let Ok(data) = event.data().dyn_into::<js_sys::JsString>() {
                let data_str: String = data.into();
                let v: Value = serde_json::from_str(&data_str).unwrap();

                let event = match web_sys::CustomEvent::new("renderEvent") {
                    Ok(event) => event,
                    Err(error) => {
                        log(&format!("Failed to create custom event: {:?}", error));
                        return;
                    }
                };

                if data_str.contains("aggTrade") {
                    if let (Some(price), Some(quantity), Some(time), Some(is_buyer_maker)) = 
                        (v["data"]["p"].as_str(), v["data"]["q"].as_str(), v["data"]["T"].as_u64(), v["data"]["m"].as_bool()) {
                        if let (Ok(price), Ok(quantity)) = (price.parse::<f64>(), quantity.parse::<f64>()) {
                            let trade = Trade { price, quantity, time, is_buyer_maker };
                            trades_buffer.push(trade);
                        }
                    }

                } else if data_str.contains("depth") {
                    if let Some(bids_array) = v["data"]["b"].as_array() {
                        if let Ok(mut bids_borrowed) = bids.write() {
                            *bids_borrowed = bids_array.iter().filter_map(|x| {
                                x[0].as_str().and_then(|price_str| price_str.parse::<f64>().ok())
                                    .and_then(|price| x[1].as_str().and_then(|quantity_str| quantity_str.parse::<f64>().ok())
                                    .map(|quantity| Some(Bid { price, quantity })).flatten())
                            }).collect();
                        } else {
                            log(&format!("bids locked on render"));
                        }
                    }     
                    if let Some(asks_array) = v["data"]["a"].as_array() {
                        if let Ok(mut asks_borrowed) = asks.write() {
                            *asks_borrowed = asks_array.iter().filter_map(|x| {
                                x[0].as_str().and_then(|price_str| price_str.parse::<f64>().ok())
                                    .and_then(|price| x[1].as_str().and_then(|quantity_str| quantity_str.parse::<f64>().ok())
                                    .map(|quantity| Some(Ask { price, quantity })).flatten())
                            }).collect();
                        } else {
                            log(&format!("asks locked on render"));
                        }
                    }

                    if current_kline_open != 0 {
                        match klines_trades.write() {
                            Ok(mut klines_trades) => {
                                let current_trades = klines_trades.entry(current_kline_open).or_insert(HashMap::new());
                                for trade in &trades_buffer {
                                    let price_as_int = ((trade.price / bucket_size).round() * bucket_size * 100.0) as i64;
                                    let key = (price_as_int, trade.is_buyer_maker);
                                    let quantity_sum = current_trades.entry(key).or_insert(0.0);
                                    *quantity_sum += trade.quantity;
                                }
                                trades_buffer.clear();
                            },
                            Err(poisoned) => {
                                log(&format!("klines_trades locked on render: {:?}", poisoned));
                            }
                        }
                    }
                    match web_sys::window() {
                        Some(window) => {
                            if let Err(error) = window.dispatch_event(&event) {
                                log(&format!("Failed to dispatch event: {:?}", error));
                            }
                        },
                        None => log("No window available"),
                    }
                    
                } else if data_str.contains("kline") {
                    if let Some(kline_data) = v["data"]["k"].as_object() {
                        let open_time = kline_data["t"].as_u64();
                        let open = kline_data["o"].as_str().and_then(|s| s.parse::<f64>().ok());
                        let high = kline_data["h"].as_str().and_then(|s| s.parse::<f64>().ok());
                        let low = kline_data["l"].as_str().and_then(|s| s.parse::<f64>().ok());
                        let close = kline_data["c"].as_str().and_then(|s| s.parse::<f64>().ok());
                        let volume = kline_data["v"].as_str().and_then(|s| s.parse::<f64>().ok());
                        let buy_volume = kline_data["V"].as_str().and_then(|s| s.parse::<f64>().ok());
                        let sell_volume = match (volume, buy_volume) {
                            (Some(volume), Some(buy_volume)) => Some(volume - buy_volume),
                            _ => None,
                        };
                        let close_time = kline_data["T"].as_u64();
                
                        if let (
                            Some(open_time), 
                            Some(open), Some(high), Some(low), Some(close), 
                            Some(buy_volume), Some(sell_volume), 
                            Some(close_time)) = (open_time, open, high, low, close, buy_volume, sell_volume, close_time) {
                            let kline = Kline {
                                open_time, 
                                open, high, low, close,
                                buy_volume, sell_volume,
                                close_time,
                            };
                            klines_ohlcv.write().unwrap().insert(open_time, kline);
                            current_kline_open = open_time;
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();
    }   

    pub fn render(&mut self) {
        match self.klines_ohlcv.try_read() {
            Ok(klines_borrowed) => {
                self.canvas_indicator_volume.render(&klines_borrowed);

                match (self.bids.try_read(), self.asks.try_read()) {
                    (Ok(bids_borrowed), Ok(asks_borrowed)) => {
                        self.canvas_orderbook.render(&klines_borrowed, &bids_borrowed, &asks_borrowed);

                        match self.klines_trades.try_read() {
                            Ok(klines_trades_borrowed) => {        
                                self.canvas_main.render(&klines_borrowed, &klines_trades_borrowed);
                            },
                            Err(e) => {
                                log(&format!("Failed to acquire lock on klines_trades during render: {}", e));
                            }
                        }
                    },
                    (Err(e), _) => {
                        log(&format!("Failed to acquire lock on bids during render: {}", e));
                    },
                    (_, Err(e)) => {
                        log(&format!("Failed to acquire lock on asks during render: {}", e));
                    }
                }
            },
            Err(e) => {
                log(&format!("Failed to acquire lock on klines during render: {}", e));
            }
        }
    }
}

pub struct CanvasOrderbook {
    canvas: web_sys::HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
}
impl CanvasOrderbook {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    canvas,
                    ctx,
                    width,
                    height,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }

    pub fn render(&mut self, klines: &RwLockReadGuard<BTreeMap<u64, Kline>>, bids: &RwLockReadGuard<Vec<Bid>>, asks: &RwLockReadGuard<Vec<Ask>>) {
        let context = &self.ctx;
        self.ctx.clear_rect(0.0, 0.0, self.width, self.height);

        let max_quantity = {
            let max_bid_quantity = bids.iter().fold(0.0, |max, x| x.quantity.max(max));
            let max_ask_quantity = asks.iter().fold(0.0, |max, x| x.quantity.max(max));
            max_bid_quantity.max(max_ask_quantity)
        };

        let width_factor = 5.0; 

        let y_max = klines.iter().map(|(_, kline)| kline.high).fold(0.0, f64::max);
        let y_min = klines.iter().map(|(_, kline)| kline.low).fold(f64::MAX, f64::min);

        if let Some(best_bid) = bids.first() {     
            context.set_fill_style(&"green".into());
            for (i, bid) in bids.iter().enumerate() {
                let x = (bid.quantity / max_quantity) * self.width as f64;
                let y = ((bid.price - y_min) / (y_max - y_min)) * self.height as f64;
                context.fill_rect(0.0, self.height as f64 - y - width_factor, x * width_factor, width_factor);
            }
        }
        if let Some(best_ask) = asks.first() {
            context.set_fill_style(&"red".into());
            for (i, ask) in asks.iter().enumerate() {
                let x = (ask.quantity / max_quantity) * self.width as f64;
                let y = ((ask.price - y_min) / (y_max - y_min)) * self.height as f64;
                context.fill_rect(0.0, self.height as f64 - y - width_factor, x * width_factor, width_factor);
            }
        }
    } 
}

pub struct CanvasMain {
    canvas: web_sys::HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    pan_x_offset: f64,
    x_zoom: f64,
}
impl CanvasMain {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    canvas,
                    ctx,
                    width,
                    height,
                    pan_x_offset: 0.0,
                    x_zoom: 30.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn render(&mut self, klines: &RwLockReadGuard<BTreeMap<u64, Kline>>, klines_trades: &RwLockReadGuard<BTreeMap<u64, HashMap<(i64, bool), f64>>>) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);

        let zoom_scale = self.x_zoom * 60.0 * 1000.0;

        let left_x = 0 - self.pan_x_offset as i64;
        let right_x = self.width as i64 - self.pan_x_offset as i64;

        match klines.iter().last() {
            Some((last_time, _)) => {
                let y_max = klines.iter().map(|(_, kline)| kline.high).fold(0.0, f64::max);
                let y_min = klines.iter().map(|(_, kline)| kline.low).fold(f64::MAX, f64::min);

                let time_difference = last_time + 60000 - zoom_scale as u64;
                let rect_width = (self.width as f64 / (&self.x_zoom/2.0)) / 6.0;
        
                for (i, (_, kline)) in klines.iter().enumerate() {
                    let x = ((kline.open_time - time_difference) as f64 / zoom_scale) * self.width;

                    let y_open = self.height as f64 * (kline.open - y_min) / (y_max - y_min);
                    let y_close = self.height as f64 * (kline.close - y_min) / (y_max - y_min);
                    let y_high = self.height as f64 * (kline.high - y_min) / (y_max - y_min);
                    let y_low = self.height as f64 * (kline.low - y_min) / (y_max - y_min);

                    if kline.open < kline.close {
                        context.set_stroke_style(&"green".into());
                    } else {
                        context.set_stroke_style(&"red".into());
                    }
                    context.begin_path();
                    context.move_to(x + rect_width, self.height - y_open);
                    context.line_to(x + rect_width, self.height - y_close);
                    context.stroke();

                    context.set_stroke_style(&"white".into());
                    context.set_line_width(1.0);
                    context.begin_path();
                    context.move_to(x, self.height - y_high);
                    context.line_to(x + (rect_width*2.0), self.height - y_high);
                    context.stroke();

                    context.begin_path();
                    context.move_to(x, self.height - y_low);
                    context.line_to(x + (rect_width*2.0), self.height - y_low);
                    context.stroke();
                }
            }
            None => {
                log(&format!("No klines"));
            }
        }
    }
}

pub struct CanvasIndicatorVolume {
    canvas: web_sys::HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    pan_x_offset: f64,
    x_zoom: f64,
}
impl CanvasIndicatorVolume {
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;

        match canvas.get_context("2d") {
            Ok(Some(context)) => {
                let ctx = context.dyn_into::<CanvasRenderingContext2d>()?;
                Ok(Self {
                    canvas,
                    ctx,
                    width,
                    height,
                    pan_x_offset: 0.0,
                    x_zoom: 30.0,
                })
            },
            Ok(None) => Err(JsValue::from_str("No 2D context available")),
            Err(error) => Err(error),
        }
    }
    pub fn render(&mut self, klines: &RwLockReadGuard<BTreeMap<u64, Kline>>) {
        let context = &self.ctx;
        context.clear_rect(0.0, 0.0, self.width, self.height);
        
        let zoom_scale = self.x_zoom * 60.0 * 1000.0;

        let left_x = 0 - self.pan_x_offset as i64;
        let right_x = self.width as i64 - self.pan_x_offset as i64;
    
        match klines.iter().last() {
            Some((last_time, _)) => {
                let max_volume = klines.iter().map(|(_, kline)| f64::max(kline.buy_volume, kline.sell_volume)).fold(0.0, f64::max);
                let time_difference = last_time + 60000 - zoom_scale as u64;
                let rect_width = (self.width as f64 / (&self.x_zoom/2.0)) / 6.0;
      
                for (i, (_, kline)) in klines.iter().enumerate() {
                    let x = ((kline.open_time - time_difference) as f64 / zoom_scale) * self.width;
                
                    let buy_height = self.height as f64 * (kline.buy_volume / max_volume);
                    let sell_height = self.height as f64 * (kline.sell_volume / max_volume);
                
                    context.set_fill_style(&"green".into());
                    context.fill_rect(x + rect_width, self.height as f64 - buy_height, rect_width, buy_height);
                
                    context.set_fill_style(&"red".into());
                    context.fill_rect(x, self.height as f64 - sell_height, rect_width, sell_height);
                }
            },
            None => {
                log(&format!("No klines"));
            }
        }
    }
}