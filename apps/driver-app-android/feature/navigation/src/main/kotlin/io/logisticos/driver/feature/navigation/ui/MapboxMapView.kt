package io.logisticos.driver.feature.navigation.ui

import android.graphics.Color as AndroidColor
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import com.mapbox.geojson.LineString
import com.mapbox.geojson.Point
import com.mapbox.maps.CameraOptions
import com.mapbox.maps.MapView
import com.mapbox.maps.Style
import com.mapbox.maps.extension.style.layers.addLayer
import com.mapbox.maps.extension.style.layers.generated.lineLayer
import com.mapbox.maps.extension.style.layers.properties.generated.LineCap
import com.mapbox.maps.extension.style.layers.properties.generated.LineJoin
import com.mapbox.maps.extension.style.sources.addSource
import com.mapbox.maps.extension.style.sources.generated.geoJsonSource
import com.mapbox.maps.plugin.annotation.annotations
import com.mapbox.maps.plugin.annotation.generated.PointAnnotationOptions
import com.mapbox.maps.plugin.annotation.generated.createPointAnnotationManager

@Composable
fun MapboxMapView(
    modifier: Modifier = Modifier,
    driverLat: Double,
    driverLng: Double,
    driverBearing: Float,
    polylineEncoded: String?,
    stopLat: Double,
    stopLng: Double
) {
    var mapViewRef by remember { mutableStateOf<MapView?>(null) }

    // MapView holds an EGL context and a native renderer — leaving it attached
    // across navigation transitions can block input dispatch to sibling Compose
    // content (e.g. bottom nav tabs). Release it explicitly on dispose.
    DisposableEffect(Unit) {
        onDispose { mapViewRef?.onDestroy() }
    }

    AndroidView(
        modifier = modifier,
        factory = { context ->
            MapView(context).also { mapView ->
                mapViewRef = mapView
                mapView.mapboxMap.loadStyle(Style.DARK) { style ->
                    if (!polylineEncoded.isNullOrEmpty()) {
                        style.addSource(geoJsonSource("route-source") {
                            geometry(decodePolyline(polylineEncoded))
                        })
                        style.addLayer(lineLayer("route-layer", "route-source") {
                            lineColor(AndroidColor.parseColor("#00E5FF"))
                            lineWidth(4.0)
                            lineCap(LineCap.ROUND)
                            lineJoin(LineJoin.ROUND)
                        })
                    }
                    val annotationManager = mapView.annotations.createPointAnnotationManager()
                    annotationManager.create(
                        PointAnnotationOptions()
                            .withPoint(Point.fromLngLat(stopLng, stopLat))
                    )
                }
            }
        },
        update = { mapView ->
            val hasDriverFix = driverLat != 0.0 || driverLng != 0.0
            val centerLat = if (hasDriverFix) driverLat else stopLat
            val centerLng = if (hasDriverFix) driverLng else stopLng
            mapView.mapboxMap.setCamera(
                CameraOptions.Builder()
                    .center(Point.fromLngLat(centerLng, centerLat))
                    .zoom(if (hasDriverFix) 15.0 else 14.0)
                    .bearing(if (hasDriverFix) driverBearing.toDouble() else 0.0)
                    .build()
            )
        }
    )
}

private fun decodePolyline(encoded: String): LineString {
    val points = mutableListOf<Point>()
    var index = 0
    var lat = 0
    var lng = 0
    while (index < encoded.length) {
        var b: Int
        var shift = 0
        var result = 0
        do {
            b = encoded[index++].code - 63
            result = result or ((b and 0x1f) shl shift)
            shift += 5
        } while (b >= 0x20)
        lat += if (result and 1 != 0) (result shr 1).inv() else result shr 1

        shift = 0
        result = 0
        do {
            b = encoded[index++].code - 63
            result = result or ((b and 0x1f) shl shift)
            shift += 5
        } while (b >= 0x20)
        lng += if (result and 1 != 0) (result shr 1).inv() else result shr 1

        points.add(Point.fromLngLat(lng / 1e5, lat / 1e5))
    }
    return LineString.fromLngLats(points)
}
