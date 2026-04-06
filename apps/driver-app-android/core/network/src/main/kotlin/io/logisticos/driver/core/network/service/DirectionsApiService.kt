package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.GET
import retrofit2.http.Query

@Serializable
data class DirectionsResponse(
    val routes: List<DirectionsRoute>,
    val status: String
)

@Serializable
data class DirectionsRoute(
    @SerialName("overview_polyline") val overviewPolyline: OverviewPolyline,
    val legs: List<DirectionsLeg>
)

@Serializable
data class OverviewPolyline(val points: String)

@Serializable
data class DirectionsLeg(
    val duration: TextValue,
    val distance: TextValue,
    val steps: List<DirectionsStep>
)

@Serializable
data class DirectionsStep(
    @SerialName("html_instructions") val htmlInstructions: String,
    val distance: TextValue,
    val duration: TextValue,
    @SerialName("end_location") val endLocation: DirectionsLatLng
)

@Serializable
data class TextValue(val text: String, val value: Int)

@Serializable
data class DirectionsLatLng(val lat: Double, val lng: Double)

interface DirectionsApiService {
    @GET("https://maps.googleapis.com/maps/api/directions/json")
    suspend fun getDirections(
        @Query("origin") origin: String,
        @Query("destination") destination: String,
        @Query("key") apiKey: String,
        @Query("mode") mode: String = "driving",
        @Query("avoid") avoid: String = "tolls"
    ): DirectionsResponse
}
